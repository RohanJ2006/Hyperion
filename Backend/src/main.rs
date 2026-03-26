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

#[tokio::main]
async fn main() {
    // Initialize the engine to handle the 50 sats + 10,000+ debris
    let initial_capacity = 15000;
    
    let state = AppState {
        engine: SimState::new(initial_capacity),
        id_to_index: HashMap::with_capacity(initial_capacity),
        current_time_unix: 1773216000.0, // Default fallback (e.g., March 12, 2026)
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
