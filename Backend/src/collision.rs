use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use twox_hash::XxHash64;
use rayon::prelude::*;

use crate::constants::*;
use crate::maths::{rk4_step, propagate_object};

// Fast hash map type alias
pub type XxHashMap<K, V> = HashMap<K, V, BuildHasherDefault<XxHash64>>;

// A predicted conjunction event returned by the grid screener.
#[derive(Debug, Clone)]
pub struct ConjunctionEvent {
    pub sat_id: u32,
    pub obj_id: u32,
    /// Unix timestamp of the Time of Closest Approach
    pub tca_unix: f64,
    /// Minimum separation at TCA (km)
    pub min_distance_km: f64,
}

#[inline(always)]
pub fn get_cell_coord(x: f64, y: f64, z: f64, cell_size: f64) -> (i32, i32, i32) {
    (
        (x / cell_size).floor() as i32,
        (y / cell_size).floor() as i32,
        (z / cell_size).floor() as i32,
    )
}

fn golden_section_search<F: Fn(f64) -> f64>(
    f: F,
    mut a: f64,
    mut b: f64,
    tol: f64,
) -> (f64, f64) {
    // φ conjugate ≈ 0.618
    let phi: f64 = (5.0_f64.sqrt() - 1.0) / 2.0;
    let mut c = b - phi * (b - a);
    let mut d = a + phi * (b - a);
    let mut fc = f(c);
    let mut fd = f(d);

    while (b - a) > tol {
        if fc < fd {
            b = d;
            d = c; fd = fc;
            c = b - phi * (b - a);
            fc = f(c);
        } else {
            a = c;
            c = d; fc = fd;
            d = a + phi * (b - a);
            fd = f(d);
        }
    }

    let t_min = (a + b) / 2.0;
    (t_min, f(t_min))
}

pub struct SpatialGrid {
    /// Default cell size (overridden dynamically in predict_conjunctions)
    pub cell_size: f64,
    pub grid: XxHashMap<(i32, i32, i32), Vec<usize>>,
}

impl SpatialGrid {
    pub fn new(cell_size: f64, capacity: usize) -> Self {
        Self {
            cell_size,
            grid: XxHashMap::with_capacity_and_hasher(capacity * 2, Default::default()),
        }
    }
    
    pub fn predict_conjunctions(
        &self,
        ids: &[u32],
        is_satellite: &[bool],
        x: &[f64], y: &[f64], z: &[f64],
        vx: &[f64], vy: &[f64], vz: &[f64],
        current_time: f64,
        prediction_window: f64,
        sps: f64,
    ) -> Vec<ConjunctionEvent> {
        let n_objects = ids.len();
        if n_objects == 0 { return Vec::new(); }

        let n_steps = ((prediction_window / sps).ceil() as usize).max(1);

        // Cell size formula (Hellwig et al. eq. 1): gc = d + v_max_LEO * sps
        // Ensures no object can skip a cell between consecutive sample steps.
        let gc = CRITICAL_CONJUNCTION_DISTANCE + 7.8 * sps;
        let mut pos: Vec<(f64, f64, f64, f64, f64, f64)> =
            vec![(0.0, 0.0, 0.0, 0.0, 0.0, 0.0); n_steps * n_objects];

        // Step 0 — current telemetry positions
        for i in 0..n_objects {
            pos[i] = (x[i], y[i], z[i], vx[i], vy[i], vz[i]);
        }

        // Steps 1..n_steps — propagate each object forward with RK4
        // Sequential over steps (each step depends on the previous),
        // but parallelised across objects within each step.
        for step in 1..n_steps {
            let prev_base = (step - 1) * n_objects;
            let curr_base = step * n_objects;

            // Safe split: we read from prev_base slice, write into curr_base slice.
            let (prev_slice, curr_slice) = pos.split_at_mut(curr_base);
            let prev_slice = &prev_slice[prev_base..];

            curr_slice[..n_objects]
                .par_iter_mut()
                .zip(prev_slice.par_iter())
                .for_each(|(out, &(px, py, pz, pvx, pvy, pvz))| {
                    *out = rk4_step(px, py, pz, pvx, pvy, pvz, sps);
                });
        }

        let candidate_pairs: Vec<(usize, usize, usize, f64)> = (0..n_steps)
            .into_par_iter()
            .flat_map(|step| {
                let base = step * n_objects;

                // Build a local grid for this time step (no locking needed)
                let mut local_grid: XxHashMap<(i32, i32, i32), Vec<usize>> =
                    XxHashMap::with_capacity_and_hasher(n_objects * 2, Default::default());

                for i in 0..n_objects {
                    let p = &pos[base + i];
                    let cell = get_cell_coord(p.0, p.1, p.2, gc);
                    local_grid.entry(cell).or_insert_with(|| Vec::with_capacity(4)).push(i);
                }

                // Check each satellite against its 26 neighbours
                let mut local_pairs: Vec<(usize, usize, usize, f64)> = Vec::new();

                for i in 0..n_objects {
                    if !is_satellite[i] { continue; }
                    let pi = &pos[base + i];
                    let home = get_cell_coord(pi.0, pi.1, pi.2, gc);

                    for dx in -1i32..=1 {
                        for dy in -1i32..=1 {
                            for dz in -1i32..=1 {
                                let neighbor = (home.0 + dx, home.1 + dy, home.2 + dz);
                                if let Some(occupants) = local_grid.get(&neighbor) {
                                    for &j in occupants {
                                        // Only unique pairs (i < j avoids double-counting)
                                        if i >= j { continue; }
                                        let pj = &pos[base + j];
                                        let dist_sq = (pi.0 - pj.0).powi(2)
                                            + (pi.1 - pj.1).powi(2)
                                            + (pi.2 - pj.2).powi(2);
                                        // Screening threshold: gc² (wider than conjunction threshold)
                                        if dist_sq < gc * gc {
                                            local_pairs.push((i, j, step, dist_sq));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                local_pairs
            })
            .collect();
       
        let mut pair_best: HashMap<(usize, usize), (usize, f64)> = HashMap::new();
        for (i, j, step, dist_sq) in candidate_pairs {
            let key = (i, j); // already guaranteed i < j
            pair_best.entry(key)
                .and_modify(|(best_step, best_dsq)| {
                    if dist_sq < *best_dsq { *best_step = step; *best_dsq = dist_sq; }
                })
                .or_insert((step, dist_sq));
        }
       
        pair_best.par_iter()
            .filter_map(|(&(i, j), &(center_step, _))| {
                let t_center = current_time + center_step as f64 * sps;
                let t_lo = (t_center - sps).max(current_time);
                let t_hi = (t_center + sps).min(current_time + prediction_window);

                // Capture initial states (at t=0) for both objects
                let (ix0, iy0, iz0, ivx0, ivy0, ivz0) = (x[i], y[i], z[i], vx[i], vy[i], vz[i]);
                let (jx0, jy0, jz0, jvx0, jvy0, jvz0) = (x[j], y[j], z[j], vx[j], vy[j], vz[j]);

                // Propagate both objects to t_lo once (avoid redundant work in the inner loop)
                let dt_lo = t_lo - current_time;
                let (ix_lo, iy_lo, iz_lo, ivx_lo, ivy_lo, ivz_lo) =
                    propagate_object(ix0, iy0, iz0, ivx0, ivy0, ivz0, dt_lo);
                let (jx_lo, jy_lo, jz_lo, jvx_lo, jvy_lo, jvz_lo) =
                    propagate_object(jx0, jy0, jz0, jvx0, jvy0, jvz0, dt_lo);

                // Distance function: propagate from t_lo to t (short integration)
                let dist_at_t = |t: f64| -> f64 {
                    let dt = t - t_lo;
                    let (ix, iy, iz, _, _, _) =
                        propagate_object(ix_lo, iy_lo, iz_lo, ivx_lo, ivy_lo, ivz_lo, dt);
                    let (jx, jy, jz, _, _, _) =
                        propagate_object(jx_lo, jy_lo, jz_lo, jvx_lo, jvy_lo, jvz_lo, dt);
                    ((ix - jx).powi(2) + (iy - jy).powi(2) + (iz - jz).powi(2)).sqrt()
                };

                // Golden-section search — 1-second tolerance is sufficient
                let (tca, min_dist) = golden_section_search(dist_at_t, t_lo, t_hi, 1.0);

                if min_dist < CRITICAL_CONJUNCTION_DISTANCE {
                    Some(ConjunctionEvent {
                        sat_id: ids[i],
                        obj_id: ids[j],
                        tca_unix: tca,
                        min_distance_km: min_dist,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn find_current_conjunctions(
        &mut self,
        ids: &[u32],
        is_satellite: &[bool],
        x: &[f64], y: &[f64], z: &[f64],
    ) -> Vec<(u32, u32)> {
        // Cell size: just 2× the collision threshold for a tight current-position check
        let gc = CRITICAL_CONJUNCTION_DISTANCE * 2.0;
        self.grid.clear();

        for i in 0..x.len() {
            let cell = get_cell_coord(x[i], y[i], z[i], gc);
            self.grid.entry(cell).or_insert_with(|| Vec::with_capacity(4)).push(i);
        }

        // Immutable borrow of grid for Rayon closure
        let grid = &self.grid;

        (0..ids.len())
            .into_par_iter()
            .flat_map(|i| {
                let mut local: Vec<(u32, u32)> = Vec::new();
                if !is_satellite[i] { return local; }
                let home = get_cell_coord(x[i], y[i], z[i], gc);

                for dx in -1i32..=1 {
                    for dy in -1i32..=1 {
                        for dz in -1i32..=1 {
                            let nb = (home.0 + dx, home.1 + dy, home.2 + dz);
                            if let Some(occ) = grid.get(&nb) {
                                for &j in occ {
                                    if i >= j { continue; }
                                    let d2 = (x[i] - x[j]).powi(2)
                                        + (y[i] - y[j]).powi(2)
                                        + (z[i] - z[j]).powi(2);
                                    if d2 < CRITICAL_CONJUNCTION_DISTANCE * CRITICAL_CONJUNCTION_DISTANCE {
                                        local.push((ids[i], ids[j]));
                                    }
                                }
                            }
                        }
                    }
                }
                local
            })
            .collect()
    }
} 
