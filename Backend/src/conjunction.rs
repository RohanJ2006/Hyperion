// Architecture:
//   1. Propagate all objects (RK4, already in physics.rs)
//   2. Insert into a sparse grid (hash map keyed by 3-D cell index)
//   3. For each non-empty cell, collect candidate pairs from the cell
//      and its 26 neighbours
//   4. Pass candidates through classical orbital filters (hybrid step):
//        a) Apogee / Perigee altitude filter
//        b) Minimum orbit-path distance filter (linear-algebra shortcut)
//        c) Time-window filter (are both objects near the node simultaneously?)
//   5. For survivors, run golden-section + Illinois bracket search (Brent-style)
//      to find the exact TCA and PCA

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use crate::maths::*;
use crate::constants::*;
// use crate::physics::*;

// This constants are adjustable

pub const CONJUNCTION_THRESHOLD_KM: f64 = 0.100;

/// Typical LEO speed used for cell-size formula (km/s).
const V_LEO: f64 = 7.5; // for us it should be 7.5km/s as 27000km/hr is given in problem statement

/// Default seconds-per-sample.  Auto-reduced when memory is tight (Section V-B).
const DEFAULT_SPS: f64 = 30.0;

/// Minimum SPS we are willing to accept before we give up reducing.
const MIN_SPS: f64 = 1.0;

/// Fraction of available RAM we are willing to use for the grid structures.
/// Kept conservative (40 %) so the rest of the process is not starved. (can change)
const MEMORY_BUDGET_FRACTION: f64 = 0.40;

/// Bytes of available system RAM (read once at startup via /proc/meminfo).
/// Falls back to 2 GiB if the file cannot be parsed.
/// One issue is if we don't find /proc/meminfo but the system has like 10gb ram, we will only consume 2gb!!! (have to fix this!!!)
fn available_ram_bytes() -> u64 {
    // Try Linux /proc/meminfo first
    if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
        for line in content.lines() {
            if line.starts_with("MemAvailable:") {
                let kb: u64 = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                if kb > 0 {
                    return kb * 1024;
                }
            }
        }
    }
    2 * 1024 * 1024 * 1024 // 2 GiB fallback
}

// public data types

/// One snapshot of an orbital object (satellite or debris).
/// Mirrors the fields already stored in SimState but as a value type so
/// threads can own their slice without lifetime gymnastics.
#[derive(Clone, Debug)]
pub struct ObjectSnapshot {
    pub id: u32,
    pub is_satellite: bool,
    pub pos: (f64, f64, f64),
    pub vel: (f64, f64, f64),
}

/// Result of a confirmed conjunction event.
// Notice how we need atleast one satellite (obviously) for this...
#[derive(Clone, Debug)]
pub struct ConjunctionEvent {
    pub satellite_id: u32,
    pub debris_id: u32,
    /// Time of Closest Approach relative to the snapshot epoch (seconds)
    pub tca_offset_s: f64,
    /// Point of Closest Approach – Euclidean distance (km)
    pub pca_km: f64,
}

// grid cell index

/// A signed 3-D grid cell address.
/// Using signed integer handles negative ECI coordinates correctly (object behind Earth)
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct CellKey(i32, i32, i32);

impl CellKey {
    #[inline]
    fn from_pos(pos: (f64, f64, f64), inv_cell: f64) -> Self {
        // Here inv_cell is 1/cell_size, we do this cause multiplication is faster than division!
        CellKey(
            (pos.0 * inv_cell).floor() as i32,
            (pos.1 * inv_cell).floor() as i32,
            (pos.2 * inv_cell).floor() as i32,
        )
    }
}

// sparse grid

/// Sparse 3-D grid backed by a HashMap.
/// Each cell stores the indices of objects that fall inside it.
/// Paper Section IV-A: we use a hash map because the simulation volume
/// (~85,000 km)³ is enormous and almost entirely empty.
struct SparseGrid {
    cell_km: f64,
    /// cell key → list of object indices into the snapshot slice
    cells: HashMap<CellKey, Vec<usize>>,
}

impl SparseGrid {
    fn new(cell_km: f64, capacity_hint: usize) -> Self {
        SparseGrid {
            cell_km,
            cells: HashMap::with_capacity(capacity_hint),
        }
    }

    /// Insert object at index `idx` with position `pos`.
    #[inline]
    fn insert(&mut self, idx: usize, pos: (f64, f64, f64)) {
        let key = CellKey::from_pos(pos, 1.0 / self.cell_km);
        self.cells.entry(key).or_default().push(idx); // if cell exists then append index, nor create new cell and add index
    }

    /// Collect all unique ordered pairs (i, j) with i < j where at least one
    /// of the two is a satellite, and both live in the same cell or adjacent
    /// cells (the 3³−1 = 26 neighbours, Section IV-A-2).
    fn candidate_pairs(&self, objects: &[ObjectSnapshot]) -> Vec<(usize, usize)> {
        let mut seen: HashMap<(usize, usize), ()> = HashMap::new();
        let mut pairs: Vec<(usize, usize)> = Vec::new();

        for (&CellKey(cx, cy, cz), cell_members) in &self.cells {
            // Collect objects from this cell and all 26 neighbours
            let mut neighbourhood: Vec<usize> = Vec::with_capacity(64);
            for dx in -1i32..=1 {
                for dy in -1i32..=1 {
                    for dz in -1i32..=1 {
                        let nk = CellKey(cx + dx, cy + dy, cz + dz);
                        if let Some(neighbours) = self.cells.get(&nk) {
                            neighbourhood.extend_from_slice(neighbours);
                        }
                    }
                }
            }

            // Pair every object in the *current cell* with every object in
            // the neighbourhood (avoids double-counting by enforcing i < j
            // and using the seen set).
            for &i in cell_members {
                for &j in &neighbourhood {
                    if i == j {
                        continue;
                    }
                    // Require at least one satellite in the pair
                    if !objects[i].is_satellite && !objects[j].is_satellite {
                        continue;
                    }
                    let key = if i < j { (i, j) } else { (j, i) };
                    if seen.insert(key, ()).is_none() {
                        pairs.push(key);
                    }
                }
            }
        }
        pairs
    }
}

// orbital filters
//
// We implement the three filters most commonly cited:
//   1. Apogee / Perigee altitude filter
//   2. Minimum orbit-path (closest-approach-on-orbit) distance filter
//   3. Time-window filter
//
// All filters are conservative: if uncertain, we keep the pair.

/// Rough orbital altitude band from an ECI position (km above Earth surface).
#[inline]
fn altitude_km(pos: (f64, f64, f64)) -> f64 {
    magnitude(pos) - RADIUS_OF_EARTH
}

/// Filter 1 – Apogee/Perigee.
/// We approximate apogee/perigee from the current state vector using the
/// vis-viva equation to get semi-major axis, then use eccentricity.
/// Returns false (reject pair) when altitude bands don't overlap within
/// the threshold.
fn apogee_perigee_filter(
    a: &ObjectSnapshot,
    b: &ObjectSnapshot,
    threshold_km: f64,
) -> bool {
    let orbital_band = |pos: (f64, f64, f64), vel: (f64, f64, f64)| -> (f64, f64) {
        let r = magnitude(pos);
        let v2 = vel.0 * vel.0 + vel.1 * vel.1 + vel.2 * vel.2;
        // Vis-viva: 1/a = 2/r - v²/μ
        let inv_a = 2.0 / r - v2 / STANDARD_GRAVITATIONAL_PARAMETER;
        if inv_a <= 0.0 {
            // Hyperbolic / escape — keep pair
            return (0.0, f64::MAX);
        }
        let sma = 1.0 / inv_a; // semi-major axis (km)

        // Specific angular momentum h = r × v
        let h = cross_product(pos, vel);
        let h_mag = magnitude(h);

        // Eccentricity vector magnitude: e = |v×h/μ - r̂|
        // We compute via the scalar form: e² = 1 + (v²/μ - 2/r)·r²·sin²(fpa) ... 
        // simpler: e = sqrt(1 - (h²/(μ·a))) clamped to [0,1)
        let e_sq = (1.0 - (h_mag * h_mag) / (STANDARD_GRAVITATIONAL_PARAMETER * sma)).max(0.0);
        let e = e_sq.sqrt().min(0.9999);

        let perigee = sma * (1.0 - e); // km from Earth centre
        let apogee  = sma * (1.0 + e);

        (perigee - RADIUS_OF_EARTH, apogee - RADIUS_OF_EARTH) // altitude
    };

    let (pa, aa) = orbital_band(a.pos, a.vel);
    let (pb, ab) = orbital_band(b.pos, b.vel);

    // Altitude bands must overlap within the threshold
    pa <= ab + threshold_km && pb <= aa + threshold_km
}

/// Filter 2 – Minimum orbit-path distance.
/// Approximates the closest distance between the two *instantaneous* velocity
/// lines (linear extrapolation from current state).  A proper implementation
/// would use the full osculating orbit geometry; this linear version is fast
/// and conservative (passes more pairs than the true filter).
fn orbit_path_filter(
    a: &ObjectSnapshot,
    b: &ObjectSnapshot,
    threshold_km: f64,
) -> bool {
    // Relative position and velocity
    let dp = (
        a.pos.0 - b.pos.0,
        a.pos.1 - b.pos.1,
        a.pos.2 - b.pos.2,
    );
    let dv = (
        a.vel.0 - b.vel.0,
        a.vel.1 - b.vel.1,
        a.vel.2 - b.vel.2,
    );

    let dv2 = dot_product(dv, dv);
    if dv2 < 1e-12 {
        // Nearly parallel trajectories — keep pair
        return true;
    }

    // Time of closest linear approach
    let t_min = -dot_product(dp, dv) / dv2;

    // Closest distance along linear path
    let min_dist = if t_min < 0.0 {
        // Already diverging — use current separation
        magnitude(dp)
    } else {
        let closest = (
            dp.0 + dv.0 * t_min,
            dp.1 + dv.1 * t_min,
            dp.2 + dv.2 * t_min,
        );
        magnitude(closest)
    };

    min_dist <= threshold_km * 20.0 // generous factor — orbital curves can differ
}

/// Filter 3 – Time window.
/// The two objects must be near the nodal crossing simultaneously.
/// We check whether the minimum linear-approach time falls within
/// a plausible window (±one orbital period).  Ultra-conservative.
fn time_window_filter(
    a: &ObjectSnapshot,
    b: &ObjectSnapshot,
    horizon_s: f64,
) -> bool {
    let dp = (
        a.pos.0 - b.pos.0,
        a.pos.1 - b.pos.1,
        a.pos.2 - b.pos.2,
    );
    let dv = (
        a.vel.0 - b.vel.0,
        a.vel.1 - b.vel.1,
        a.vel.2 - b.vel.2,
    );
    let dv2 = dot_product(dv, dv);
    if dv2 < 1e-12 {
        return true;
    }
    let t_min = -dot_product(dp, dv) / dv2;
    // Keep if the closest linear approach happens inside our prediction horizon
    t_min >= -60.0 && t_min <= horizon_s
}

// Brent / golden-section TCA search
//
// Section IV-C of the paper: Brent's algorithm combines golden-section
// reliability with interpolation performance.  We implement a clean
// golden-section + parabolic interpolation (Brent 1973) for the 1-D
// minimisation of distance(t) on an interval [lo, hi].

const GOLDEN_RATIO: f64 = 0.381_966_011_250_105; // (3 - √5) / 2
const BRENT_TOL: f64 = 1e-4; // tolerance in seconds
const BRENT_MAX_ITER: usize = 100;

/// Linearly extrapolate an ECI state forward by `dt` seconds.
/// Good enough for the short Brent search intervals (seconds to minutes).
#[inline]
fn propagate_linear(pos: (f64, f64, f64), vel: (f64, f64, f64), dt: f64) -> (f64, f64, f64) {
    (pos.0 + vel.0 * dt, pos.1 + vel.1 * dt, pos.2 + vel.2 * dt)
}

/// Distance between two objects at time `t` (linear propagation from snapshot).
#[inline]
fn dist_at(a: &ObjectSnapshot, b: &ObjectSnapshot, t: f64) -> f64 {
    let (pa, _) = propagate_rk4_to(a.pos, a.vel, t);
    let (pb, _) = propagate_rk4_to(b.pos, b.vel, t);
    let d = (pa.0 - pb.0, pa.1 - pb.1, pa.2 - pb.2);
    magnitude(d)
}

/// Brent minimisation of `dist_at(a, b, t)` on [lo, hi].
/// Returns (t_min, dist_min).
fn brent_tca(a: &ObjectSnapshot, b: &ObjectSnapshot, lo: f64, hi: f64) -> (f64, f64) {
    let mut a_b = lo;
    let mut b_b = hi;

    // Initial golden-section points
    let mut x = a_b + GOLDEN_RATIO * (b_b - a_b);
    let mut w = x;
    let mut v = x;
    let mut fx = dist_at(a, b, x);
    let mut fw = fx;
    let mut fv = fx;
    let mut d: f64 = 0.0;
    let mut e: f64 = 0.0;

    for _ in 0..BRENT_MAX_ITER {
        let midpoint = 0.5 * (a_b + b_b);
        let tol1 = BRENT_TOL * x.abs() + 1e-10;
        let tol2 = 2.0 * tol1;

        if (x - midpoint).abs() <= tol2 - 0.5 * (b_b - a_b) {
            return (x, fx);
        }

        // Try parabolic interpolation
        let mut use_golden = true;
        if e.abs() > tol1 {
            let r = (x - w) * (fx - fv);
            let q = (x - v) * (fx - fw);
            let p_num = (x - v) * q - (x - w) * r;
            let p_den = 2.0 * (q - r);
            let p;
            let p_den_abs;
            if p_den > 0.0 {
                p = p_num;
                p_den_abs = p_den;
            } else {
                p = -p_num;
                p_den_abs = -p_den;
            }
            if p.abs() < (0.5 * e * p_den_abs).abs()
                && p > p_den_abs * (a_b - x)
                && p < p_den_abs * (b_b - x)
            {
                d = p / p_den_abs;
                use_golden = false;
                let u = x + d;
                if (u - a_b) < tol2 || (b_b - u) < tol2 {
                    d = if midpoint >= x { tol1 } else { -tol1 };
                }
            }
        }
        if use_golden {
            e = if x >= midpoint {
                a_b - x
            } else {
                b_b - x
            };
            d = GOLDEN_RATIO * e;
        }

        let u = x + if d.abs() >= tol1 { d } else if d > 0.0 { tol1 } else { -tol1 };
        let fu = dist_at(a, b, u);

        if fu <= fx {
            if u < x { b_b = x; } else { a_b = x; }
            v = w; fv = fw;
            w = x; fw = fx;
            x = u; fx = fu;
        } else {
            if u < x { a_b = u; } else { b_b = u; }
            if fu <= fw || (w - x).abs() < 1e-10 {
                v = w; fv = fw;
                w = u; fw = fu;
            } else if fu <= fv || (v - x).abs() < 1e-10 || (v - w).abs() < 1e-10 {
                v = u; fv = fu;
            }
        }
    }
    (x, fx)
}

// Replace the single brent_tca call in the Brent phase with this:

fn brent_tca_multi(
    a: &ObjectSnapshot,
    b: &ObjectSnapshot,
    horizon_s: f64,
) -> (f64, f64) {
    // Approximate orbital period from vis-viva: T = 2π√(a³/μ)
    // Use average altitude of the two objects as a rough semimajor axis
    let r_a = magnitude(a.pos);
    let r_b = magnitude(b.pos);
    let sma_avg = (r_a + r_b) / 2.0;
    let period_s = 2.0 * std::f64::consts::PI * (sma_avg.powi(3) / STANDARD_GRAVITATIONAL_PARAMETER).sqrt();

    // Search in half-period windows (two crossings per period)
    let window = period_s / 2.0;
    let n_windows = (horizon_s / window).ceil() as usize;

    let mut best_tca = 0.0_f64;
    let mut best_pca = f64::MAX;

    for w in 0..n_windows {
        let lo = w as f64 * window;
        let hi = (lo + window).min(horizon_s);
        let (tca, pca) = brent_tca(a, b, lo, hi);
        if pca < best_pca {
            best_pca = pca;
            best_tca = tca;
        }
    }

    (best_tca, best_pca)
}

// memory budget helper

/// Estimate memory (bytes) needed for one grid pass over `n_objects` objects.
/// Per-object cost in the grid: ~80 bytes (HashMap entry + cell vec element).
/// Per-candidate-pair cost: ~32 bytes.
fn estimate_grid_memory_bytes(n_objects: usize) -> u64 {
    let grid_bytes = (n_objects as u64) * 80;
    // Rough upper bound: every object pairs with ~10 neighbours on average
    let pair_bytes = (n_objects as u64) * 10 * 32;
    grid_bytes + pair_bytes
}

/// Choose the largest SPS that fits comfortably in the memory budget.
/// Smaller SPS → more time steps → larger total grid memory; but each
/// individual grid pass stays the same size.  The budget concerns how many
/// grids we materialise *simultaneously* (here: 1, sequential processing).
fn choose_sps(n_objects: usize) -> f64 {
    let ram = available_ram_bytes();
    let budget = (ram as f64 * MEMORY_BUDGET_FRACTION) as u64;
    let single_pass = estimate_grid_memory_bytes(n_objects);

    if single_pass > budget {
        // Even one pass is tight — warn and proceed anyway with minimum SPS
        eprintln!(
            "[conjunction] Warning: estimated grid memory {} MB exceeds budget {} MB. \
             Proceeding with min SPS.",
            single_pass / (1024 * 1024),
            budget / (1024 * 1024)
        );
        return MIN_SPS;
    }
    DEFAULT_SPS
}

// main public function

/// Run one full hybrid conjunction screening pass.
///
/// # Arguments
/// * `objects`    – current snapshot of every tracked object
/// * `horizon_s`  – prediction horizon in seconds (e.g. 86400 for 24 h)
///
/// # Returns
/// A Vec of [`ConjunctionEvent`] sorted by PCA (closest first).
pub fn hybrid_conjunction_screening(
    objects: &[ObjectSnapshot],
    horizon_s: f64,
) -> Vec<ConjunctionEvent> {

    let n = objects.len();
    if n < 2 {
        return Vec::new();
    }

    let sps = choose_sps(n);

    // Cell size formula (Eq. 1):  gc = d + V_LEO * sps
    let cell_km = CONJUNCTION_THRESHOLD_KM + V_LEO * sps;

    let n_steps = (horizon_s / sps).ceil() as usize;

    eprintln!(
        "[conjunction] hybrid screening: {} objects, horizon {:.0} s, \
         sps {:.0} s, cell {:.2} km, steps {}",
        n, horizon_s, sps, cell_km, n_steps
    );

    // 1. Collect candidate pairs across all time steps
    //
    // For the hybrid variant we use larger cells (SPS is bigger than the
    // grid-only variant) and compensate with orbital filters.
    // We merge candidates across all steps into a single dedup'd set.

    // Thread-safe accumulator for candidate index pairs
    let candidates: Arc<Mutex<HashMap<(usize, usize), ()>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // Determine thread count
    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);

    let chunk_size = (n_steps / n_threads).max(1);
    let objects_arc = Arc::new(objects.to_vec());

    thread::scope(|s| {
        for thread_id in 0..n_threads {
            let step_start = thread_id * chunk_size;
            // Guard: if n_steps < n_threads some threads get an empty range
            if step_start >= n_steps {
                continue;
            }
            let step_end = if thread_id == n_threads - 1 {
                n_steps
            } else {
                (step_start + chunk_size).min(n_steps)
            };

            let objs = Arc::clone(&objects_arc);
            let cands = Arc::clone(&candidates);
            let cell = cell_km;

            s.spawn(move || {
                // Each thread builds its own local set, then bulk-merges
                let mut local: HashMap<(usize, usize), ()> = HashMap::new();

                for step in step_start..step_end {
                    let t = step as f64 * sps;
                    let mut grid = SparseGrid::new(cell, objs.len());

                    // Insert propagated positions into the grid
                    for (idx, obj) in objs.iter().enumerate() {
                        let pos = propagate_linear(obj.pos, obj.vel, t);
                        grid.insert(idx, pos);
                    }

                    // Collect candidate pairs from the grid
                    for pair in grid.candidate_pairs(&objs) {
                        local.insert(pair, ());
                    }
                }

                // Merge into global set
                let mut guard = cands.lock().unwrap();
                for pair in local.into_keys() {
                    guard.insert(pair, ());
                }
            });
        }
    });

    let candidate_pairs: Vec<(usize, usize)> = {
        let guard = candidates.lock().unwrap();
        guard.keys().copied().collect()
    };

    eprintln!(
        "[conjunction] grid produced {} candidate pairs",
        candidate_pairs.len()
    );

    // 2. Orbital filter chain (the "hybrid" part)
    //
    // Paper Section III: the hybrid variant passes grid candidates through
    // classical filter chains to prune false positives before the expensive
    // Brent TCA search.

    let threshold = CONJUNCTION_THRESHOLD_KM;

    let filtered: Vec<(usize, usize)> = candidate_pairs
        .into_iter()
        .filter(|&(i, j)| {
            let a = &objects[i];
            let b = &objects[j];
            apogee_perigee_filter(a, b, threshold)
                && orbit_path_filter(a, b, threshold)
                && time_window_filter(a, b, horizon_s)
        })
        .collect();

    eprintln!(
        "[conjunction] after orbital filters: {} pairs remain",
        filtered.len()
    );

    // 3. Brent TCA / PCA for surviving pairs
    //
    // Paper Section IV-C: pairs are independent, so we run them in parallel.
    // The search interval is the full prediction horizon; Brent's algorithm
    // converges in O(log²(range/tol)) iterations.

    if filtered.is_empty() {
        eprintln!("[conjunction] no pairs survived filters — no conjunctions");
        return Vec::new();
    }

    let results: Arc<Mutex<Vec<ConjunctionEvent>>> = Arc::new(Mutex::new(Vec::new()));

    // Split filtered pairs across threads
    let pairs_arc = Arc::new(filtered);

    thread::scope(|s| {
        let pair_chunk = (pairs_arc.len() / n_threads).max(1);

        for thread_id in 0..n_threads {
            let start = thread_id * pair_chunk;
            // Guard: fewer pairs than threads
            if start >= pairs_arc.len() {
                continue;
            }
            let end = if thread_id == n_threads - 1 {
                pairs_arc.len()
            } else {
                (start + pair_chunk).min(pairs_arc.len())
            };

            let objs = Arc::clone(&objects_arc);
            let res = Arc::clone(&results);
            let pairs = Arc::clone(&pairs_arc);

            s.spawn(move || {
                let mut local_events: Vec<ConjunctionEvent> = Vec::new();

                for &(i, j) in &pairs[start..end] {
                    let a = &objs[i];
                    let b = &objs[j];

                    let (tca, pca) = brent_tca_multi(a, b, horizon_s);

                    // Only report if PCA is below (or reasonably close to) threshold.
                    // Keep a generous factor so the operator can see near-misses too.
                    if pca <= threshold * 50.0 {
                        let (sat_id, deb_id) = if a.is_satellite {
                            (a.id, b.id)
                        } else {
                            (b.id, a.id)
                        };
                        local_events.push(ConjunctionEvent {
                            satellite_id: sat_id,
                            debris_id: deb_id,
                            tca_offset_s: tca,
                            pca_km: pca,
                        });
                    }
                }

                let mut guard = res.lock().unwrap();
                guard.extend(local_events);
            });
        }
    });

    let mut events = Arc::try_unwrap(results)
        .unwrap()
        .into_inner()
        .unwrap();

    // Sort by PCA ascending (closest first)
    events.sort_by(|a, b| a.pca_km.partial_cmp(&b.pca_km).unwrap());

    eprintln!(
        "[conjunction] screening complete: {} conjunction events (PCA ≤ {:.1} km)",
        events.len(),
        threshold * 50.0
    );

    events
}

// convenience wrapper

/// Build [`ObjectSnapshot`] slices from the parallel-vector layout used by
/// [`crate::physics::SimState`] and run a screening pass.
///
/// ```ignore
/// let events = screen_from_sim_state(&state, 86400.0);
/// ```
pub fn screen_from_sim_state(
    id: &[u32],
    is_satellite: &[bool],
    x: &[f64], y: &[f64], z: &[f64],
    vx: &[f64], vy: &[f64], vz: &[f64],
    horizon_s: f64,
) -> Vec<ConjunctionEvent> {
    let objects: Vec<ObjectSnapshot> = (0..id.len())
        .map(|i| ObjectSnapshot {
            id: id[i],
            is_satellite: is_satellite[i],
            pos: (x[i], y[i], z[i]),
            vel: (vx[i], vy[i], vz[i]),
        })
        .collect();

    hybrid_conjunction_screening(&objects, horizon_s)
}

// unit tests

#[cfg(test)]
mod tests {
    use super::*;

    fn circular_orbit_snapshot(id: u32, altitude_km: f64, phase_deg: f64) -> ObjectSnapshot {
        let r = RADIUS_OF_EARTH + altitude_km;
        let v = (STANDARD_GRAVITATIONAL_PARAMETER / r).sqrt();
        let phase = phase_deg.to_radians();
        ObjectSnapshot {
            id,
            is_satellite: id < 100,
            pos: (r * phase.cos(), r * phase.sin(), 0.0),
            vel: (-v * phase.sin(), v * phase.cos(), 0.0),
        }
    }

    #[test]
    fn test_cell_key_basic() {
        let key = CellKey::from_pos((100.0, 200.0, -50.0), 1.0 / 25.0);
        assert_eq!(key, CellKey(4, 8, -2));
    }

    #[test]
    fn test_no_objects() {
        let events = hybrid_conjunction_screening(&[], 3600.0);
        assert!(events.is_empty());
    }

    #[test]
    fn test_well_separated_orbits_no_conjunction() {
        // Two satellites 500 km apart in altitude — should find no critical conjunction
        let objects = vec![
            circular_orbit_snapshot(1, 400.0, 0.0),
            circular_orbit_snapshot(2, 900.0, 0.0),
        ];
        let events = hybrid_conjunction_screening(&objects, 3600.0);
        let critical: Vec<_> = events
            .iter()
            .filter(|e| e.pca_km < CONJUNCTION_THRESHOLD_KM)
            .collect();
        assert!(critical.is_empty(), "Expected no critical conjunctions");
    }

    #[test]
    fn test_close_pair_detected() {
        // Satellite and debris almost co-located
        let sat = ObjectSnapshot {
            id: 1,
            is_satellite: true,
            pos: (7000.0, 0.0, 0.0),
            vel: (0.0, 7.5, 0.0),
        };
        let deb = ObjectSnapshot {
            id: 1001,
            is_satellite: false,
            pos: (7000.05, 0.0, 0.0), // 50 m away
            vel: (0.0, 7.5, 0.05),
        };
        let events = hybrid_conjunction_screening(&[sat, deb], 600.0);
        assert!(
            !events.is_empty(),
            "Expected at least one conjunction event for near-collocated objects"
        );
        assert!(events[0].pca_km < CONJUNCTION_THRESHOLD_KM);
    }

    #[test]
    fn test_apogee_perigee_filter_rejects_far_pair() {
        let high = circular_orbit_snapshot(1, 35786.0, 0.0); // GEO
        let low  = circular_orbit_snapshot(1001, 400.0, 0.0); // LEO
        // Bands don't overlap within 0.1 km threshold
        assert!(!apogee_perigee_filter(&high, &low, CONJUNCTION_THRESHOLD_KM));
    }

    #[test]
    fn test_brent_finds_minimum() {
        // Two objects heading toward each other, should converge near t=30s
        let a = ObjectSnapshot {
            id: 1, is_satellite: true,
            pos: (7000.0, 0.0, 0.0), vel: (0.0, 0.1, 0.0),
        };
        let b = ObjectSnapshot {
            id: 2, is_satellite: false,
            pos: (7000.003, 0.0, 0.0), vel: (0.0, -0.1, 0.0),
        };
        let (tca, pca) = brent_tca(&a, &b, 0.0, 3600.0);
        assert!(tca >= 0.0 && tca <= 3600.0);
        assert!(pca < 10.0, "Expected PCA < 10 km, got {}", pca);
    }
}
