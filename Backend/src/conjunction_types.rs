// conjunction_types.rs
//
// Shared data types used by both conjunction.rs (orchestrator) and
// conjunction_cpu.rs (CPU backend).  Keeping them here breaks the
// circular import that would arise if conjunction_cpu imported from
// conjunction or vice-versa.

/// One snapshot of a tracked object (satellite or debris).
#[derive(Clone, Debug)]
pub struct ObjectSnapshot {
    pub id:           u32,
    pub is_satellite: bool,
    /// ECI position (km)
    pub pos: (f64, f64, f64),
    /// ECI velocity (km/s)
    pub vel: (f64, f64, f64),
}

/// A confirmed (or near-miss) conjunction event returned by the screener.
#[derive(Clone, Debug)]
pub struct ConjunctionEvent {
    /// ID of the active satellite in the pair
    pub satellite_id:  u32,
    /// ID of the debris / other object
    pub debris_id:     u32,
    /// Seconds from snapshot epoch to Time of Closest Approach
    pub tca_offset_s:  f64,
    /// Point of Closest Approach — Euclidean distance (km)
    pub pca_km:        f64,
}

/// Critical conjunction threshold from the problem statement (km).
pub const CONJUNCTION_THRESHOLD_KM: f64 = 0.100;
