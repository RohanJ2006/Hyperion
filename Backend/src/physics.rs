use std::thread;
use crate::constants::*;
use crate::math::*;

// Cut the string id into integer
#[inline(always)]
pub fn parse_api_id(string_id: &str) -> (u32, bool) {
    let is_satellite = string_id.starts_with("SAT");
    let numeric_id = string_id.split('-')
        .last()
        .unwrap_or("0")
        .parse::<u32>()
        .unwrap_or(0);

    (numeric_id, is_satellite)
}

// Concantenate the integer id into string
#[inline(always)]
pub fn format_api_id(numeric_id: u32, is_satellite: bool) -> String {
    if is_satellite {
        format!("SAT-{:04}", numeric_id)
    } else {
        format!("DEB-{:04}", numeric_id)
    }
}

// API data structure
#[derive(Clone)]
pub struct SimState {
    pub id: Vec<u32>,
    pub is_satellite: Vec<bool>,
    pub mass: Vec<f64>,

    // Position vector
    pub x: Vec<f64>, pub y: Vec<f64>, pub z: Vec<f64>,

    // Velocity vector
    pub vx: Vec<f64>, pub vy: Vec<f64>, pub vz: Vec<f64>,

    // Nominal target state vector
    pub nx: Vec<f64>, pub ny: Vec<f64>, pub nz: Vec<f64>,
}

// Implement the definition of struct
impl SimState {
    // Define the size of each property
    pub fn new(capacity: usize) -> Self {
        Self {
            id: Vec::with_capacity(capacity), is_satellite: Vec::with_capacity(capacity), mass: Vec::with_capacity(capacity),

            x: Vec::with_capacity(capacity), y: Vec::with_capacity(capacity), z: Vec::with_capacity(capacity),

            vx: Vec::with_capacity(capacity), vy: Vec::with_capacity(capacity), vz: Vec::with_capacity(capacity),

            nx: Vec::with_capacity(capacity), ny: Vec::with_capacity(capacity), nz: Vec::with_capacity(capacity)
        }
    }

    // Push the objects to the struct 
    pub fn push_object(
        &mut self,
        numeric_id: u32, is_satellite: bool, mass: f64,
        x: f64, y: f64, z: f64,
        vx: f64, vy: f64, vz: f64,
        nx: f64, ny: f64, nz: f64,
    ) {
        self.id.push(numeric_id); self.is_satellite.push(is_satellite); self.mass.push(mass);

        self.x.push(x); self.y.push(y); self.z.push(z);

        self.vx.push(vx); self.vy.push(vy); self.vz.push(vz);

        self.nx.push(nx); self.ny.push(ny); self.nz.push(nz);
    }

    // Propagate using RK4 and CPU core threading
    pub fn propagate(&mut self, dt: f64) {
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
        let total_length = self.x.len();
        if total_length == 0 {return; }

        // Global bound checks to save runtime size checks
        assert_eq!(self.y.len(), total_length);
        assert_eq!(self.z.len(), total_length);
        assert_eq!(self.vx.len(), total_length);
        assert_eq!(self.vy.len(), total_length);
        assert_eq!(self.vz.len(), total_length);

        let chunk_size = (total_length / threads).max(1);
        let mut x_chunks = self.x.chunks_mut(chunk_size);
        let mut y_chunks = self.y.chunks_mut(chunk_size);
        let mut z_chunks = self.z.chunks_mut(chunk_size);
        let mut vx_chunks = self.vx.chunks_mut(chunk_size);
        let mut vy_chunks = self.vy.chunks_mut(chunk_size);
        let mut vz_chunks = self.vz.chunks_mut(chunk_size);

        thread::scope(|s| {
            let iter = x_chunks.zip(&mut y_chunks).zip(&mut z_chunks)
                .zip(&mut vx_chunks).zip(&mut vy_chunks).zip(&mut vz_chunks);

            for (((((x_slice, y_slice), z_slice), vx_slice), vy_slice), vz_slice) in iter {
                s.spawn(move|| {
                    let dt2 = dt / 2.0; 
                    let dt6 = dt / 6.0;
                    let chunk_length = x_slice.len();

                    // Local bound checks to save runtime checks
                    assert_eq!(y_slice.len(), chunk_length);
                    assert_eq!(z_slice.len(), chunk_length);
                    assert_eq!(vx_slice.len(), chunk_length);
                    assert_eq!(vy_slice.len(), chunk_length);
                    assert_eq!(vz_slice.len(), chunk_length);

                    // RK4 maths
                    for i in 0..chunk_length {
                        let r0 = (x_slice[i], y_slice[i], z_slice[i]);
                        let v0 = (vx_slice[i], vy_slice[i], vz_slice[i]);

                        // First probe at the start (t = 0)
                        let a1 = j2_acceleration(r0.0, r0.1, r0.2);

                        // Jump halfway (t = dt/2) using a1 and v0
                        let r2 = (r0.0 + v0.0 * dt2, r0.1 + v0.1 * dt2, r0.2 + v0.2 * dt2);
                        let v2 = (v0.0 + a1.0 * dt2, v0.1 + a1.1 * dt2, v0.2 + a1.2 * dt2);

                        // Second probe at the halfway point using r2
                        let a2 = j2_acceleration(r2.0, r2.1, r2.2);

                        // Jump halfway (t = dt/2) using the updated v2/a2
                        let r3 = (r0.0 + v2.0 * dt2, r0.1 + v2.1 * dt2, r0.2 + v2.2 * dt2);
                        let v3 = (v0.0 + a2.0 * dt2, v0.1 + a2.1 * dt2, v0.2 + a2.2 * dt2);

                        // Third probe at the halfway point using r3
                        let a3 = j2_acceleration(r3.0, r3.1, r3.2);

                        // Jump to the end (t = dt) using the refined v3/a3
                        let r4 = (r0.0 + v3.0 * dt, r0.1 + v3.1 * dt, r0.2 + v3.2 * dt);
                        let v4 = (v0.0 + a3.0 * dt, v0.1 + a3.1 * dt, v0.2 + a3.2 * dt);

                        // Final probe at the end point using r4
                        let a4 = j2_acceleration(r4.0, r4.1, r4.2);

                        // Weighted Average Update
                        x_slice[i]  += dt6 * (v0.0 + 2.0 * v2.0 + 2.0 * v3.0 + v4.0);
                        y_slice[i]  += dt6 * (v0.1 + 2.0 * v2.1 + 2.0 * v3.1 + v4.1);
                        z_slice[i]  += dt6 * (v0.2 + 2.0 * v2.2 + 2.0 * v3.2 + v4.2);

                        vx_slice[i] += dt6 * (a1.0 + 2.0 * a2.0 + 2.0 * a3.0 + a4.0);
                        vy_slice[i] += dt6 * (a1.1 + 2.0 * a2.1 + 2.0 * a3.1 + a4.1);
                        vz_slice[i] += dt6 * (a1.2 + 2.0 * a2.2 + 2.0 * a3.2 + a4.2);
                    }
                });
            }
        });
    }
