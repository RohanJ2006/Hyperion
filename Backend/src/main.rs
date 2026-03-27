mod api;
mod constants;
mod math;
mod models;
mod physics;

use axum::{
    routing::{get, post},
    Router,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use api::*;
use physics::SimState;

// ** The central state container shared across all asynchronous API threads **
pub struct AppState {
    pub engine: SimState,
    // Maps the numeric ID to its exact index in the SoA vectors for O(1) updates
    pub id_to_index: HashMap<u32, usize>,
    // Tracks current simulation time as a Unix Timestamp (seconds)
    pub current_time_unix: f64,
}

pub type SharedState = Arc<RwLock<AppState>>;

// Ground Stations Data
// Format: (Latitude, Longitude, Elevation in meters, Min Elevation Angle)
const GROUND_STATIONS: &[(f64, f64, f64, f64)] = &[
    (13.0333, 77.5167, 820.0, 5.0),     // GS-001: ISTRAC_Bengaluru
    (78.2297, 15.4077, 400.0, 5.0),     // GS-002: Svalbard_Sat_Station
    (35.4266, -116.8900, 1000.0, 10.0), // GS-003: Goldstone_Tracking
    (-53.1500, -70.9167, 30.0, 5.0),    // GS-004: Punta_Arenas
    (28.5450, 77.1926, 225.0, 15.0),    // GS-005: IIT_Delhi_Ground_Node
    (-77.8463, 166.6682, 10.0, 5.0),    // GS-006: McMurdo_Station
];

#[tokio::main]
async fn main() {
    // Initialize the engine to handle the 50 sats + 10,000+ debris
    let initial_capacity = 15000;
    
    let state = AppState {
        engine: SimState::new(initial_capacity),
        id_to_index: HashMap::with_capacity(initial_capacity),
        // Default fallback (e.g., March 12, 2026)
        // incase we get simulate_step before any telemetry data
        // we fall back to this date instead of 1 Jan, 1970 (unix timestamp of 0)
        // Here March 12, 2026 is used cause the PS has this timestamp across all its
        // api request. Anyway we update this time when encountered with any telemetry
        // data as we have to keep both the grader universe and our simulation in sync.
        current_time_unix: 1773216000.0,
    };
    
    let shared_state: SharedState = Arc::new(RwLock::new(state));

    let app = Router::new()
        .route("/api/telemetry", post(ingest_telemetry))
        .route("/api/maneuver/schedule", post(schedule_maneuver))
        .route("/api/simulate/step", post(simulate_step))
        .route("/api/visualization/snapshot", get(get_snapshot))
        .with_state(shared_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await.unwrap();
    println!("Autonomous Constellation Manager API running on 0.0.0.0:8000");
    
    axum::serve(listener, app).await.unwrap();
}
