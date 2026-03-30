use serde::{Deserialize, Serialize};

// Common
#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub struct Vec3 { pub x: f64, pub y: f64, pub z: f64 }

// Telemetry Ingestion ***
#[derive(Debug, Deserialize)]
pub struct TelemetryPayload {
    pub timestamp: String,
    pub objects: Vec<SpaceObject>,
}

#[derive(Debug, Deserialize)]
pub struct SpaceObject {
    pub id: String,
    #[serde(rename = "type")] // rust has spl meaning for type
    pub obj_type: String,
    pub r: Vec3,
    pub v: Vec3,
}

#[derive(Debug, Serialize)]
pub struct TelemetryResponse {
    pub status: String,
    pub processed_count: usize,
    pub active_cdm_warnings: usize,
}

// *** 4.2 Maneuver Scheduling ***
// NOTE: Rust prefers snake_case but our api uses camelCase thus we use rename functionality of serde
#[derive(Debug, Deserialize)]
pub struct ManeuverPayload {
    #[serde(rename = "satelliteId")]
    pub satellite_id: String,
    pub maneuver_sequence: Vec<BurnCommand>,
}

#[derive(Debug, Deserialize)]
pub struct BurnCommand {
    pub burn_id: String,
    #[serde(rename = "burnTime")]
    pub burn_time: String,
    #[serde(rename = "deltaV_vector")]
    pub delta_v_vector: Vec3,
}

#[derive(Debug, Serialize)]
pub struct ManeuverValidation {
    pub ground_station_los: bool,
    pub sufficient_fuel: bool,
    pub projected_mass_remaining_kg: f64,
}

// Response Code (202 Accepted)
#[derive(Debug, Serialize)]
pub struct ManeuverResponse {
    pub status: String,
    pub validation: ManeuverValidation,
}

// Simulation Step
#[derive(Debug, Deserialize)]
pub struct StepPayload {
    pub step_seconds: f64,
}

#[derive(Debug, Serialize)]
pub struct StepResponse {
    pub status: String,
    pub new_timestamp: String,
    pub collisions_detected: usize,
    pub maneuvers_executed: usize,
}

// 6.3 Visualization Snapshot
#[derive(Debug, Serialize)]
pub struct SnapshotResponse {
    pub timestamp: String,
    pub satellites: Vec<SatStatus>,
    pub debris_cloud: Vec<(String, f64, f64, f64)>,
}

#[derive(Debug, Serialize)]
pub struct SatStatus {
    pub id: String,
    pub lat: f64,
    pub lon: f64,
    pub fuel_kg: f64,
    pub status: String,
}
