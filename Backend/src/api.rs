use axum::{
    extract::State,
    body::Bytes,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use simd_json::from_slice;

use crate::models::*;
use crate::physics::{parse_api_id, format_api_id};
use crate::math::eci_to_geodetic;
use crate::SharedState;

/// Converts an ISO8601 string to a f64 Unix Timestamp
fn parse_timestamp(iso_str: &str) -> f64 {
    match iso_str.parse::<DateTime<Utc>>() {
        Ok(dt) => dt.timestamp_millis() as f64 / 1000.0,
        Err(_) => 0.0,
    }
}

/// POST /api/telemetry
pub async fn ingest_telemetry(
    State(state): State<SharedState>,
    bytes: Bytes,
) -> impl IntoResponse {
    let mut payload_bytes = bytes.to_vec(); 
    let payload: Result<TelemetryPayload, _> = from_slice(&mut payload_bytes);

    match payload {
        Ok(data) => {
            let mut app = state.write().await;
            app.current_time_unix = parse_timestamp(&data.timestamp);
            
            for obj in &data.objects {
                let (numeric_id, is_satellite) = parse_api_id(&obj.id);
                
                // If object exists, update its state in O(1) time
                if let Some(&index) = app.id_to_index.get(&numeric_id) {
                    app.engine.x[index] = obj.r.x;
                    app.engine.y[index] = obj.r.y;
                    app.engine.z[index] = obj.r.z;
                    app.engine.vx[index] = obj.v.x;
                    app.engine.vy[index] = obj.v.y;
                    app.engine.vz[index] = obj.v.z;
                } else {
                    // Object is new, push it to the vectors
                    let initial_mass = if is_satellite { 550.0 } else { 0.0 };
                    let current_len = app.engine.id.len();
                    
                    app.engine.push_object(
                        numeric_id, is_satellite, initial_mass,
                        obj.r.x, obj.r.y, obj.r.z,
                        obj.v.x, obj.v.y, obj.v.z,
                        obj.r.x, obj.r.y, obj.r.z // Initialize nominal target to starting position (might have to change this)
                    );
                    app.id_to_index.insert(numeric_id, current_len);
                }
            }
            
            let response = TelemetryResponse {
                status: "ACK".to_string(),
                processed_count: data.objects.len(),
                active_cdm_warnings: 0, // Placeholder until TCA module is built
            };
            (StatusCode::OK, Json(response))
        }
        // Might have to change this section! Don't know how to respond exactly when encountered bad request
        Err(_) => {
            (StatusCode::BAD_REQUEST, Json(TelemetryResponse {
                status: "ERR_PARSE".to_string(),
                processed_count: 0,
                active_cdm_warnings: 0,
            }))
        }
    }
}

/// POST /api/simulate/step
pub async fn simulate_step(
    State(state): State<SharedState>,
    Json(payload): Json<StepPayload>,
) -> (StatusCode, Json<StepResponse>) {
    let mut app = state.write().await;
    
    app.engine.propagate(payload.step_seconds);
    
    // Advance global time
    app.current_time_unix += payload.step_seconds;

    // Convert back to ISO8601 string format
    let new_time_iso = DateTime::<Utc>::from_timestamp(app.current_time_unix as i64, 0)
        .unwrap_or_default()
        .to_rfc3339();

    let response = StepResponse {
        status: "STEP_COMPLETE".to_string(),
        new_timestamp: new_time_iso,
        collisions_detected: 0, // Placeholder, have to add collision detection code!
        maneuvers_executed: 0,  // Placeholder, have to add maneuver code!
    };

    (StatusCode::OK, Json(response))
}

/// GET /api/visualization/snapshot
pub async fn get_snapshot(
    State(state): State<SharedState>,
) -> (StatusCode, Json<SnapshotResponse>) {
    let app = state.read().await;
    let time_unix = app.current_time_unix;
    let engine = &app.engine;

    let mut satellites = Vec::new();
    let mut debris_cloud = Vec::new();

    for i in 0..engine.id.len() {
        let eci_pos = (engine.x[i], engine.y[i], engine.z[i]);
        
        // Convert to Geodetic using your math.rs functions
        let (lat_rad, lon_rad, alt_km) = eci_to_geodetic(eci_pos, time_unix);
        
        let lat_deg = lat_rad.to_degrees();
        let lon_deg = lon_rad.to_degrees();
        let str_id = format_api_id(engine.id[i], engine.is_satellite[i]);

        if engine.is_satellite[i] {
            satellites.push(SatStatus {
                id: str_id,
                lat: lat_deg,
                lon: lon_deg,
                fuel_kg: engine.mass[i] - 500.0, // Wet mass minus 500kg dry mass
                status: "NOMINAL".to_string(),
            });
        } else {
            // Highly compressed tuple format as asked in PS
            debris_cloud.push((str_id, lat_deg, lon_deg, alt_km));
        }
    }

    let timestamp_iso = DateTime::<Utc>::from_timestamp(time_unix as i64, 0)
        .unwrap_or_default()
        .to_rfc3339();

    let response = SnapshotResponse {
        timestamp: timestamp_iso,
        satellites,
        debris_cloud,
    };

    (StatusCode::OK, Json(response))
}

/// POST /api/maneuver/schedule
pub async fn schedule_maneuver(
    State(_state): State<SharedState>,
    Json(_payload): Json<ManeuverPayload>,
) -> (StatusCode, Json<ManeuverResponse>) {
    // We will build the Burn execution logic here next!
    let response = ManeuverResponse {
        status: "SCHEDULED".to_string(),
        validation: ManeuverValidation {
            ground_station_los: true,
            sufficient_fuel: true,
            projected_mass_remaining_kg: 50.0,
        }
    };
    (StatusCode::ACCEPTED, Json(response))
}
