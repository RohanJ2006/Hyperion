mod api;
mod constants;
mod maths;
mod models;
mod physics;
mod collision;

use axum::{
    routing::{get, post},
    Router,
};

use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::api::*;
use crate::physics::SimState;
use crate::collision::{SpatialGrid, ConjunctionEvent};
use crate::constants::API_PORT;

/// Central shared state for all async API handlers.
pub struct AppState {
    pub engine: SimState,
    pub radar: SpatialGrid,
    pub id_to_index: HashMap<u32, usize>,
    pub current_time_unix: f64,
    /// Cache of active CDM warnings from the last 24-hour prediction run.
    /// Updated on every simulate/step call.
    pub active_conjunctions: Vec<ConjunctionEvent>,
}

pub type SharedState = Arc<RwLock<AppState>>;

#[tokio::main]
async fn main() {

    // Pre-allocate for 50 satellites + 10,000+ debris objects with headroom
    let initial_capacity = 15_000;

    let state = AppState {
        engine: SimState::new(initial_capacity),
        // Cell size will be overridden dynamically in predict_conjunctions.
        // The value here is only used for find_current_conjunctions (instant check).
        radar: SpatialGrid::new(0.2, initial_capacity),
        id_to_index: HashMap::with_capacity(initial_capacity),
        // Fallback time (March 12, 2026) in case simulate/step arrives before telemetry.
        // Matches the timestamp used throughout the problem statement examples.
        current_time_unix: 1_773_216_000.0,
        active_conjunctions: Vec::new(),
    };

    let shared_state: SharedState = Arc::new(RwLock::new(state));

    let cors = CorsLayer::new()
    .allow_origin(Any)
    .allow_methods(Any)
    .allow_headers(Any);

    let app = Router::new()
        .route("/api/telemetry",               post(ingest_telemetry))
        .route("/api/maneuver/schedule",        post(schedule_maneuver))
        .route("/api/simulate/step",            post(simulate_step))
        .route("/api/visualization/snapshot",  get(get_snapshot))
        .fallback_service(ServeDir::new("../frontend/dist"))
        .layer(cors)
        .with_state(shared_state);

    let bind_addr = format!("0.0.0.0:{}", API_PORT);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();

    println!("Backend is live and listening on http://{}", bind_addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.expect("Failed to install Ctrl+C handler");
        })
        .await
        .unwrap();
}

