use crate::constants::*;
use crate::maths::*;
use rayon::prelude::*;

// Parses an API string ID like "SAT-Alpha-04" or "DEB-99421" into (numeric_id, is_satellite).
// Numeric ID is taken from the last dash-separated segment.
#[inline(always)]
pub fn parse_api_id(string_id: &str) -> (u32, bool) {
    let is_satellite = string_id.starts_with("SAT");
    let numeric_id = string_id
        .split('-')
        .last()
        .unwrap_or("0")
        .parse::<u32>()
        .unwrap_or(0);
    (numeric_id, is_satellite)
}

#[derive(Clone, Debug)]
pub struct ScheduleManeuver {
    pub satellite_id: u32,
    pub burn_time_unix: f64,
    pub dv_x: f64, // km/s in ECI
    pub dv_y: f64,
    pub dv_z: f64,
}

#[derive(Clone)]
pub struct SimState {
    // Identity
    pub id: Vec<u32>,
    pub string_id: Vec<String>, // Original API string (e.g. "SAT-Alpha-04") for response formatting
    pub is_satellite: Vec<bool>,
    pub mass: Vec<f64>,         // Current wet mass (kg)

    // Current state — position (km) and velocity (km/s) in ECI J2000
    pub x: Vec<f64>, pub y: Vec<f64>, pub z: Vec<f64>,
    pub vx: Vec<f64>, pub vy: Vec<f64>, pub vz: Vec<f64>,

    // Nominal orbital slot — propagated via two-body only (no J2) to track ideal position
    pub nx: Vec<f64>, pub ny: Vec<f64>, pub nz: Vec<f64>,
    pub nvx: Vec<f64>, pub nvy: Vec<f64>, pub nvz: Vec<f64>,

    pub maneuver_queue: Vec<ScheduleManeuver>,
    pub last_burn_time: Vec<f64>,
    pub is_eol: Vec<bool>, // True once EOL deorbit has been scheduled
}

impl SimState {
    pub fn new(capacity: usize) -> Self {
        Self {
            id: Vec::with_capacity(capacity),
            string_id: Vec::with_capacity(capacity),
            is_satellite: Vec::with_capacity(capacity),
            mass: Vec::with_capacity(capacity),
            x:  Vec::with_capacity(capacity), y:  Vec::with_capacity(capacity), z:  Vec::with_capacity(capacity),
            vx: Vec::with_capacity(capacity), vy: Vec::with_capacity(capacity), vz: Vec::with_capacity(capacity),
            nx:  Vec::with_capacity(capacity), ny:  Vec::with_capacity(capacity), nz:  Vec::with_capacity(capacity),
            nvx: Vec::with_capacity(capacity), nvy: Vec::with_capacity(capacity), nvz: Vec::with_capacity(capacity),
            maneuver_queue: Vec::new(),
            last_burn_time: Vec::with_capacity(capacity),
            is_eol: Vec::with_capacity(capacity),
        }
    }

    pub fn push_object(
        &mut self,
        numeric_id: u32,
        str_id: String,
        is_satellite: bool,
        mass: f64,
        x: f64, y: f64, z: f64,
        vx: f64, vy: f64, vz: f64,
        // Nominal slot initialised to the same state as the real state.
        // For debris this is unused. For satellites, two-body propagation will
        // advance nx/ny/nz each step to track the ideal unperturbed orbit.
        nx: f64, ny: f64, nz: f64,
        nvx: f64, nvy: f64, nvz: f64,
    ) {
        self.id.push(numeric_id);
        self.string_id.push(str_id);
        self.is_satellite.push(is_satellite);
        self.mass.push(mass);
        self.x.push(x);   self.y.push(y);   self.z.push(z);
        self.vx.push(vx); self.vy.push(vy); self.vz.push(vz);
        self.nx.push(nx);  self.ny.push(ny);  self.nz.push(nz);
        self.nvx.push(nvx); self.nvy.push(nvy); self.nvz.push(nvz);
        self.last_burn_time.push(0.0);
        self.is_eol.push(false);
    }

    // Apply one RK4 step (J2) to ALL objects in parallel using Rayon.
    fn rk4_all_objects(&mut self, dt: f64) {
        // Compute new states in parallel from immutable borrows, then write back.
        let new_states: Vec<_> = self.x.par_iter()
            .zip(self.y.par_iter())
            .zip(self.z.par_iter())
            .zip(self.vx.par_iter())
            .zip(self.vy.par_iter())
            .zip(self.vz.par_iter())
            .map(|(((((x, y), z), vx), vy), vz)| {
                rk4_step(*x, *y, *z, *vx, *vy, *vz, dt)
            })
            .collect();

        for (i, (nx, ny, nz, nvx, nvy, nvz)) in new_states.into_iter().enumerate() {
            self.x[i] = nx; self.y[i] = ny; self.z[i] = nz;
            self.vx[i] = nvx; self.vy[i] = nvy; self.vz[i] = nvz;
        }
    }

    // Advance nominal slots using unperturbed two-body mechanics (satellites only).
    // This keeps nx/ny/nz tracking the ideal Keplerian reference orbit,
    // so station-keeping drift is measured against a moving target, not a fixed point.
    fn propagate_nominal_slots(&mut self, dt: f64) {
        let new_nominal: Vec<Option<_>> = self.nx.par_iter()
            .zip(self.ny.par_iter())
            .zip(self.nz.par_iter())
            .zip(self.nvx.par_iter())
            .zip(self.nvy.par_iter())
            .zip(self.nvz.par_iter())
            .zip(self.is_satellite.par_iter())
            .map(|((((((nx, ny), nz), nvx), nvy), nvz), &is_sat)| {
                if !is_sat { return None; }
                Some(two_body_step(*nx, *ny, *nz, *nvx, *nvy, *nvz, dt))
            })
            .collect();

        for (i, result) in new_nominal.into_iter().enumerate() {
            if let Some((nnx, nny, nnz, nnvx, nnvy, nnvz)) = result {
                self.nx[i] = nnx; self.ny[i] = nny; self.nz[i] = nnz;
                self.nvx[i] = nnvx; self.nvy[i] = nnvy; self.nvz[i] = nnvz;
            }
        }
    }

    // Main simulation advance: propagates physics with sub-stepping for accuracy,
    // and interleaves maneuver burns at their exact scheduled times.
    //
    // Returns the number of burns that were executed.
    pub fn propagate_and_execute(&mut self, dt: f64, window_start: f64) -> usize {
        // 30-second sub-steps keep RK4 error below ~1 m even on 1-hour ticks
        const SUB_STEP: f64 = 30.0;
        let n_sub = ((dt / SUB_STEP).ceil() as usize).max(1);
        let actual_dt = dt / n_sub as f64;

        let mut current_time = window_start;
        let mut total_executed = 0;

        for _ in 0..n_sub {
            let step_end = current_time + actual_dt;

            // Collect maneuvers that fall within this sub-step, sorted by burn time
            let mut to_execute: Vec<ScheduleManeuver> = self.maneuver_queue
                .iter()
                .filter(|m| m.burn_time_unix >= current_time && m.burn_time_unix < step_end)
                .cloned()
                .collect();
            to_execute.sort_by(|a, b| a.burn_time_unix.partial_cmp(&b.burn_time_unix).unwrap());

            let mut t = current_time;

            for maneuver in &to_execute {
                // Propagate to burn time
                let dt_to_burn = maneuver.burn_time_unix - t;
                if dt_to_burn > 1e-6 {
                    self.rk4_all_objects(dt_to_burn);
                    self.propagate_nominal_slots(dt_to_burn);
                }

                // Apply impulsive burn to the correct satellite
                if let Some(idx) = self.id.iter().zip(self.is_satellite.iter())
                    .position(|(&id, &is_sat)| id == maneuver.satellite_id && is_sat)
                {
                    let dv_mag = (maneuver.dv_x.powi(2) + maneuver.dv_y.powi(2) + maneuver.dv_z.powi(2)).sqrt();
                    let fuel_burned = calculate_fuel_burn(self.mass[idx], dv_mag);
                    // Clamp: mass never drops below dry mass
                    self.mass[idx] = (self.mass[idx] - fuel_burned).max(DRY_MASS);
                    self.vx[idx] += maneuver.dv_x;
                    self.vy[idx] += maneuver.dv_y;
                    self.vz[idx] += maneuver.dv_z;
                    self.last_burn_time[idx] = maneuver.burn_time_unix;
                    total_executed += 1;
                }

                t = maneuver.burn_time_unix;
            }

            // Propagate the remainder of the sub-step
            let remainder = step_end - t;
            if remainder > 1e-6 {
                self.rk4_all_objects(remainder);
                self.propagate_nominal_slots(remainder);
            }

            // Remove executed maneuvers from the queue
            // (identify by exact burn_time match rather than re-borrowing during retain)
            {
                let executed_keys: std::collections::HashSet<u64> = to_execute.iter()
                    .map(|m| (m.burn_time_unix * 1000.0) as u64)
                    .collect();
                self.maneuver_queue.retain(|m| {
                    !executed_keys.contains(&((m.burn_time_unix * 1000.0) as u64))
                });
            }

            // Check EOL and schedule deorbit burns if needed
            self.check_and_schedule_eol(step_end);

            current_time = step_end;
        }

        total_executed
    }

    // Check each satellite's fuel. If below the EOL threshold and not yet flagged,
    // schedule a retrograde deorbit burn to lower perigee into a graveyard orbit.
    fn check_and_schedule_eol(&mut self, current_time: f64) {
        // Collect EOL candidates first to avoid borrow conflict with maneuver_queue
        let eol_candidates: Vec<(usize, (f64, f64, f64), (f64, f64, f64), u32, f64)> = self.id
            .iter()
            .enumerate()
            .filter_map(|(i, &id)| {
                if !self.is_satellite[i] || self.is_eol[i] { return None; }
                let fuel = self.mass[i] - DRY_MASS;
                if fuel > EOL_FUEL_THRESHOLD { return None; }
                // Skip if already has a queued maneuver
                if self.maneuver_queue.iter().any(|m| m.satellite_id == id) { return None; }
                let pos = (self.x[i], self.y[i], self.z[i]);
                let vel = (self.vx[i], self.vy[i], self.vz[i]);
                Some((i, pos, vel, id, fuel))
            })
            .collect();

        for (i, pos, vel, id, fuel) in eol_candidates {
            self.is_eol[i] = true;

            // Retrograde burn: use remaining fuel up to MAX_THRUST_DELTA
            // dv = Isp * g0 * ln(m_wet / m_dry), capped at 15 m/s
            let dv_mag_km = (SPECIFIC_IMPULSE * STANDARD_GRAVITY_KM
                * ((DRY_MASS + fuel) / DRY_MASS).ln())
                .min(MAX_THRUST_DELTA);

            // Convert retrograde RTN to ECI
            let dv_eci = rtn_to_eci(pos, vel, (0.0, -dv_mag_km, 0.0));

            self.maneuver_queue.push(ScheduleManeuver {
                satellite_id: id,
                burn_time_unix: current_time + COMMUNICATION_LATENCY as f64,
                dv_x: dv_eci.0,
                dv_y: dv_eci.1,
                dv_z: dv_eci.2,
            });
        }
    }

    /// Returns IDs of all satellites currently outside their 10 km station-keeping box.
    pub fn check_station_keeping(&self) -> Vec<u32> {
        self.id.iter().enumerate()
            .filter(|&(i, _)| {
                if !self.is_satellite[i] { return false; }
                let drift_sq = (self.x[i] - self.nx[i]).powi(2)
                    + (self.y[i] - self.ny[i]).powi(2)
                    + (self.z[i] - self.nz[i]).powi(2);
                drift_sq > DRIFT_TOLERANCE * DRIFT_TOLERANCE
            })
            .map(|(_, &id)| id)
            .collect()
    }
}
