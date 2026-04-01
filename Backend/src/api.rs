use crate::AppState;
use axum::{
    extract::State,
    body::Bytes,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use std::time::Duration;
use tokio::time::sleep;
use chrono::{DateTime, Utc};
use simd_json::from_slice;

use crate::models::*;
use crate::physics::{parse_api_id, ScheduleManeuver};
use crate::maths::{eci_to_geodetic, eci_to_ecef, calculate_elevation_angle, calculate_fuel_burn, calculate_gmst, geodetic_to_ecef, rtn_to_eci};
use crate::conjunction::{screen_from_sim_state, ObjectSnapshot};
use crate::constants::*;
use crate::SharedState;

// Converts an ISO 8601 timestamp string to a Unix timestamp (f64 seconds).
// Returns Err with a message on failure — callers must handle this properly.
fn parse_timestamp(iso_str: &str) -> Result<f64, String> {
    iso_str.parse::<DateTime<Utc>>()
        .map(|dt| dt.timestamp_millis() as f64 / 1000.0)
        .map_err(|_| format!("Invalid timestamp: {}", iso_str))
}

pub async fn ingest_telemetry(
    State(state): State<SharedState>,
    bytes: Bytes,
) -> impl IntoResponse {
    let mut payload_bytes = bytes.to_vec();
    let payload: Result<TelemetryPayload, _> = from_slice(&mut payload_bytes);

    match payload {
        Ok(data) => {
            let timestamp = match parse_timestamp(&data.timestamp) {
                Ok(t) => t,
                Err(e) => {
                    return (StatusCode::BAD_REQUEST, Json(TelemetryResponse {
                        status: "ERR_TIMESTAMP".to_string(),
                        processed_count: 0,
                        active_cdm_warnings: 0,
                    }));
                }
            };

            let mut app = state.write().await;
            app.current_time_unix = timestamp;

            for obj in &data.objects {
                let (numeric_id, is_satellite) = parse_api_id(&obj.id);

                // obj_type is used as a secondary validation check
                let type_says_satellite = obj.obj_type.eq_ignore_ascii_case("SATELLITE");
                let is_satellite = is_satellite || type_says_satellite;

                if let Some(&index) = app.id_to_index.get(&numeric_id) {
                    // Update existing object
                    app.engine.x[index] = obj.r.x;
                    app.engine.y[index] = obj.r.y;
                    app.engine.z[index] = obj.r.z;
                    app.engine.vx[index] = obj.v.x;
                    app.engine.vy[index] = obj.v.y;
                    app.engine.vz[index] = obj.v.z;
                } else {
                    // Register new object
                    let initial_mass = if is_satellite { INITIAL_WET_MASS } else { 0.0 };
                    let current_len = app.engine.id.len();

                    app.engine.push_object(
                        numeric_id,
                        obj.id.clone(),
                        is_satellite,
                        initial_mass,
                        obj.r.x, obj.r.y, obj.r.z,
                        obj.v.x, obj.v.y, obj.v.z,
                        // Nominal slot starts at the first reported position + velocity.
                        // For satellites, two-body propagation will advance this each tick.
                        obj.r.x, obj.r.y, obj.r.z,
                        obj.v.x, obj.v.y, obj.v.z,
                    );
                    app.id_to_index.insert(numeric_id, current_len);
                }
            }

            let active_warnings = app.active_conjunctions.len();

            evaluate_autonomous_evasion(&mut app);

            let response = TelemetryResponse {
                status: "ACK".to_string(),
                processed_count: data.objects.len(),
                active_cdm_warnings: active_warnings,
            };
            (StatusCode::OK, Json(response))
        }
        Err(e) => {
            (StatusCode::BAD_REQUEST, Json(TelemetryResponse {
                status: "ERR_PARSE".to_string(),
                processed_count: 0,
                active_cdm_warnings: 0,
            }))
        }
    }
}

pub async fn simulate_step(
    State(state): State<SharedState>,
    Json(payload): Json<StepPayload>,
) -> (StatusCode, Json<StepResponse>) {
    let mut app = state.write().await;

    let start_time = app.current_time_unix;
    let end_time = start_time + payload.step_seconds;

    // 1. Propagate physics and execute queued maneuvers (interleaved at correct times)
    let maneuvers_executed = app.engine.propagate_and_execute(payload.step_seconds, start_time);

    // 2. Instant collision check (actual collisions that occurred this tick)
    // We pass 0.0 for the horizon because we only care about right now
    let current_collisions = screen_from_sim_state(
        &app.engine.id,
        &app.engine.is_satellite,
        &app.engine.x, &app.engine.y, &app.engine.z,
        &app.engine.vx, &app.engine.vy, &app.engine.vz,
        0.0,
    );

    // Filter to only include collisions under the 100ms threshold
    let actual_crashes: Vec<_> = current_collisions.into_iter()
        .filter(|c| c.pca_km <= 0.100)
        .collect();

    // 3. Run 24-hour predictive conjunction assessment
    // (updates the active CDM warning cache used by telemetry responses)
    {
        let engine = &app.engine;
        app.active_conjunctions = screen_from_sim_state(
            &engine.id,
            &engine.is_satellite,
            &engine.x, &engine.y, &engine.z,
            &engine.vx, &engine.vy, &engine.vz,
            PREDICTION_WINDOW as f64,
        );
    }

    // 4. Station-keeping audit
    let out_of_box = app.engine.check_station_keeping();
    if !out_of_box.is_empty() {
    }

    app.current_time_unix = end_time;

    let new_time_iso = DateTime::<Utc>::from_timestamp(end_time as i64, 0)
        .unwrap_or_default()
        .to_rfc3339();

    evaluate_autonomous_evasion(&mut app);

    (StatusCode::OK, Json(StepResponse {
        status: "STEP_COMPLETE".to_string(),
        new_timestamp: new_time_iso,
        collisions_detected: actual_crashes.len(),
        maneuvers_executed,
    }))
}

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
        let (lat_rad, lon_rad, alt_km) = eci_to_geodetic(eci_pos, time_unix);
        let lat_deg = lat_rad.to_degrees();
        let lon_deg = lon_rad.to_degrees();

        // Use the original string ID for correct API format
        let str_id = engine.string_id[i].clone();

        let drift_sq = (engine.x[i] - engine.nx[i]).powi(2)
            + (engine.y[i] - engine.ny[i]).powi(2)
            + (engine.z[i] - engine.nz[i]).powi(2);
        let status = if engine.is_eol[i] {
            "EOL".to_string()
        } else if drift_sq > DRIFT_TOLERANCE * DRIFT_TOLERANCE {
            "OUT_OF_SLOT".to_string()
        } else {
            "NOMINAL".to_string()
        };

        if engine.is_satellite[i] {
            satellites.push(SatStatus {
                id: str_id,
                lat: lat_deg,
                lon: lon_deg,
                fuel_kg: (engine.mass[i] - DRY_MASS).max(0.0),
                status,
            });
        } else {
            debris_cloud.push((str_id, lat_deg, lon_deg, alt_km));
        }
    }

    let timestamp_iso = DateTime::<Utc>::from_timestamp(time_unix as i64, 0)
        .unwrap_or_default()
        .to_rfc3339();

    (StatusCode::OK, Json(SnapshotResponse {
        timestamp: timestamp_iso,
        satellites,
        debris_cloud,
    }))
}

pub async fn schedule_maneuver(
    State(state): State<SharedState>,
    Json(payload): Json<ManeuverPayload>,
) -> (StatusCode, Json<ManeuverResponse>) {
    let mut app = state.write().await;

    let (numeric_id, is_sat) = parse_api_id(&payload.satellite_id);
    if !is_sat {
        return (StatusCode::BAD_REQUEST, Json(ManeuverResponse {
            status: "REJECTED: Target is not a satellite".to_string(),
            validation: ManeuverValidation { ground_station_los: false, sufficient_fuel: false, projected_mass_remaining_kg: 0.0 },
        }));
    }

    let index = match app.id_to_index.get(&numeric_id) {
        Some(&i) => i,
        None => return (StatusCode::NOT_FOUND, Json(ManeuverResponse {
            status: "REJECTED: Satellite not found".to_string(),
            validation: ManeuverValidation { ground_station_los: false, sufficient_fuel: false, projected_mass_remaining_kg: 0.0 },
        })),
    };

    // ---- Line-of-sight check (using current satellite position) ----
    let sat_eci = (app.engine.x[index], app.engine.y[index], app.engine.z[index]);
    let gmst = calculate_gmst(app.current_time_unix);
    let sat_ecef = eci_to_ecef(sat_eci, gmst);

    let mut has_los = false;
    for &(gs_lat_deg, gs_lon_deg, gs_alt_m, gs_min_elev) in GROUND_STATIONS {
        let gs_lat_rad = gs_lat_deg.to_radians();
        let gs_lon_rad = gs_lon_deg.to_radians();
        let gs_ecef = geodetic_to_ecef(gs_lat_rad, gs_lon_rad, gs_alt_m / 1000.0);
        let elevation = calculate_elevation_angle(sat_ecef, gs_lat_rad, gs_lon_rad, gs_ecef);
        if elevation >= gs_min_elev {
            has_los = true;
            break;
        }
    }

    if !has_los {
        return (StatusCode::BAD_REQUEST, Json(ManeuverResponse {
            status: "REJECTED: Communications blackout".to_string(),
            validation: ManeuverValidation { ground_station_los: false, sufficient_fuel: true, projected_mass_remaining_kg: app.engine.mass[index] },
        }));
    }

    // ---- Validate and queue each burn ----
    let mut current_mass = app.engine.mass[index];
    let mut last_burn = app.engine.last_burn_time[index];
    let mut new_maneuvers: Vec<ScheduleManeuver> = Vec::new();

    for burn_cmd in &payload.maneuver_sequence {
        let burn_time = match parse_timestamp(&burn_cmd.burn_time) {
            Ok(t) => t,
            Err(e) => return (StatusCode::BAD_REQUEST, Json(ManeuverResponse {
                status: format!("REJECTED: {}", e),
                validation: ManeuverValidation { ground_station_los: has_los, sufficient_fuel: false, projected_mass_remaining_kg: current_mass },
            })),
        };

        // 10-second communication latency: burn cannot be scheduled in the past or too soon
        if burn_time < app.current_time_unix + COMMUNICATION_LATENCY as f64 {
            return (StatusCode::BAD_REQUEST, Json(ManeuverResponse {
                status: format!("REJECTED: Burn '{}' violates 10s latency constraint", burn_cmd.burn_id),
                validation: ManeuverValidation { ground_station_los: has_los, sufficient_fuel: true, projected_mass_remaining_kg: current_mass },
            }));
        }

        // Thruster cooldown between successive burns
        if last_burn != 0.0 && burn_time - last_burn < THRUSTER_COOLDOWN as f64 {
            return (StatusCode::BAD_REQUEST, Json(ManeuverResponse {
                status: format!("REJECTED: Burn '{}' violates 600s thruster cooldown", burn_cmd.burn_id),
                validation: ManeuverValidation { ground_station_los: has_los, sufficient_fuel: false, projected_mass_remaining_kg: current_mass },
            }));
        }

        // Maximum Δv per burn: 15 m/s = 0.015 km/s
        let dv = burn_cmd.delta_v_vector;
        let dv_mag = (dv.x.powi(2) + dv.y.powi(2) + dv.z.powi(2)).sqrt();
        if dv_mag > MAX_THRUST_DELTA {
            return (StatusCode::BAD_REQUEST, Json(ManeuverResponse {
                status: format!(
                    "REJECTED: Burn '{}' exceeds max Δv ({:.4} > {:.4} km/s)",
                    burn_cmd.burn_id, dv_mag, MAX_THRUST_DELTA
                ),
                validation: ManeuverValidation { ground_station_los: has_los, sufficient_fuel: true, projected_mass_remaining_kg: current_mass },
            }));
        }

        // Fuel check using Tsiolkovsky
        let fuel_burned = calculate_fuel_burn(current_mass, dv_mag);
        if current_mass - fuel_burned < DRY_MASS {
            return (StatusCode::BAD_REQUEST, Json(ManeuverResponse {
                status: format!("REJECTED: Burn '{}' — insufficient fuel", burn_cmd.burn_id),
                validation: ManeuverValidation { ground_station_los: has_los, sufficient_fuel: false, projected_mass_remaining_kg: current_mass },
            }));
        }

        current_mass -= fuel_burned;
        last_burn = burn_time;

        new_maneuvers.push(ScheduleManeuver {
            satellite_id: numeric_id,
            burn_time_unix: burn_time,
            dv_x: dv.x,
            dv_y: dv.y,
            dv_z: dv.z,
        });
    }

    // All burns validated — commit to the queue
    app.engine.maneuver_queue.extend(new_maneuvers);

    (StatusCode::ACCEPTED, Json(ManeuverResponse {
        status: "SCHEDULED".to_string(),
        validation: ManeuverValidation {
            ground_station_los: has_los,
            sufficient_fuel: true,
            projected_mass_remaining_kg: current_mass,
        },
    }))
}

// WEBSOCKET UPGRADE HANDLER
pub async fn ws_telemetry_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    // Upgrades the HTTP GET request to a persistent WebSocket connection
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

// THE BINARY STREAMING LOOP
async fn handle_socket(mut socket: WebSocket, state: SharedState) {
    loop {
        // 1. Lock the state and build the flat Float64 buffer
        let payload: Vec<f64> = {
            let app = state.read().await;
            let engine = &app.engine;
            let time_unix = app.current_time_unix;
            
            // Pre-allocate vector: (Total Objects) * 7 fields per object
            let mut buffer = Vec::with_capacity(engine.id.len() * 7);

            for i in 0..engine.id.len() {
                let eci_pos = (engine.x[i], engine.y[i], engine.z[i]);
                let (lat_rad, lon_rad, alt_km) = eci_to_geodetic(eci_pos, time_unix);

                // Field 1: ID (Numeric part)
                buffer.push(engine.id[i] as f64);
                
                // Field 2: Latitude in degrees
                buffer.push(lat_rad.to_degrees());
                
                // Field 3: Longitude in degrees
                buffer.push(lon_rad.to_degrees());
                
                // Field 4: Altitude in km
                buffer.push(alt_km);
                
                // Field 5: X (Placeholder for WASM Canvas math)
                buffer.push(0.0);
                
                // Field 6: Y (Placeholder for WASM Canvas math)
                buffer.push(0.0);
                
                // Field 7: Type (1.0 for SATELLITE, 0.0 for DEBRIS)
                buffer.push(if engine.is_satellite[i] { 1.0 } else { 0.0 });
            }
            
            buffer
        };

        // 2. Convert the Vec<f64> into raw bytes (Vec<u8>) for transmission
        // JavaScript Float64Arrays natively use Little-Endian byte order on modern hardware
        let mut byte_buffer = Vec::with_capacity(payload.len() * 8);
        for val in payload {
            byte_buffer.extend_from_slice(&val.to_le_bytes());
        }

        // 3. Blast the binary message to the frontend
        if socket.send(Message::Binary(byte_buffer)).await.is_err() {
            println!("Client disconnected from telemetry stream.");
            break; // Exit the loop if the frontend closes the connection
        }

        // Stream at 10 Hz (100ms sleep). You can lower this to 33ms for 30 FPS!
        sleep(Duration::from_millis(100)).await;
    }
}

/// Evaluates future conjunctions and schedules evasive maneuvers if a critical threshold is breached.
/// Implements a 15-second simulation-time throttle to prevent CPU starvation during high-frequency telemetry ingestion.
fn evaluate_autonomous_evasion(app: &mut AppState) {
    // Throttle: Only run the heavy prediction if 15 simulation seconds have passed
    if app.current_time_unix - app.last_screening_time < 15.0 {
        return;
    }

    let engine = &app.engine;
    
    // 1. Run the predictive screening
    app.active_conjunctions = screen_from_sim_state(
        &engine.id,
        &engine.is_satellite,
        &engine.x, &engine.y, &engine.z,
        &engine.vx, &engine.vy, &engine.vz,
        PREDICTION_WINDOW as f64,
    );

    app.last_screening_time = app.current_time_unix;

    // 2. Process critical threats
    let mut emergency_burns = Vec::new();
    
    for event in &app.active_conjunctions {
        if event.pca_km <= 0.100 {
            let sat_already_dodging = emergency_burns.iter().any(|b: &ScheduleManeuver| b.satellite_id == event.satellite_id) || 
                                      app.engine.maneuver_queue.iter().any(|b| b.satellite_id == event.satellite_id);

            if !sat_already_dodging {
                let numeric_id = event.satellite_id;
                if let Some(&index) = app.id_to_index.get(&numeric_id) {
                    if app.engine.mass[index] > DRY_MASS {
                        println!("[SYSTEM] Critical trajectory detected: Sat {} vs Deb {}. Initial PCA: {:.3} km.", 
                                 numeric_id, event.debris_id, event.pca_km);
                        
                        let pos_eci = (app.engine.x[index], app.engine.y[index], app.engine.z[index]);
                        let vel_eci = (app.engine.vx[index], app.engine.vy[index], app.engine.vz[index]);

                        // 1. Calculate both Delta-V vectors in the global ECI frame
                        let (dv_x_pro, dv_y_pro, dv_z_pro) = crate::maths::rtn_to_eci(pos_eci, vel_eci, (0.0, MAX_THRUST_DELTA, 0.0));
                        let (dv_x_ret, dv_y_ret, dv_z_ret) = crate::maths::rtn_to_eci(pos_eci, vel_eci, (0.0, -MAX_THRUST_DELTA, 0.0));

                        // 2. Fetch the debris state vectors to test against
                        let deb_index = app.id_to_index.get(&event.debris_id).expect("Debris missing from index");
                        let deb_snap = ObjectSnapshot {
                            id: event.debris_id,
                            is_satellite: false,
                            pos: (app.engine.x[*deb_index], app.engine.y[*deb_index], app.engine.z[*deb_index]),
                            vel: (app.engine.vx[*deb_index], app.engine.vy[*deb_index], app.engine.vz[*deb_index]),
                        };

                        // 3. Create virtual satellite clones with the proposed Delta-V applied
                        let sat_pro = ObjectSnapshot {
                            id: numeric_id,
                            is_satellite: true,
                            pos: pos_eci,
                            vel: (vel_eci.0 + dv_x_pro, vel_eci.1 + dv_y_pro, vel_eci.2 + dv_z_pro),
                        };

                        let sat_ret = ObjectSnapshot {
                            id: numeric_id,
                            is_satellite: true,
                            pos: pos_eci,
                            vel: (vel_eci.0 + dv_x_ret, vel_eci.1 + dv_y_ret, vel_eci.2 + dv_z_ret),
                        };

                        // 4. Run the Trial Ephemeris through the Brent algorithm
                        let (_, pca_pro) = crate::conjunction::brent_tca_multi(&sat_pro, &deb_snap, PREDICTION_WINDOW as f64);
                        let (_, pca_ret) = crate::conjunction::brent_tca_multi(&sat_ret, &deb_snap, PREDICTION_WINDOW as f64);

                        // 5. Select the maneuver that yields the safest distance
                        let (best_dv_x, best_dv_y, best_dv_z, final_pca, burn_type) = if pca_pro >= pca_ret {
                            (dv_x_pro, dv_y_pro, dv_z_pro, pca_pro, "PROGRADE")
                        } else {
                            (dv_x_ret, dv_y_ret, dv_z_ret, pca_ret, "RETROGRADE")
                        };

                        println!("[SYSTEM] Selected {} burn. Improved miss distance to {:.3} km.", burn_type, final_pca);

                        // Schedule the optimal burn 15 seconds from current time
                        let burn_time = app.current_time_unix + 15.0;

                        emergency_burns.push(ScheduleManeuver {
                            satellite_id: numeric_id,
                            burn_time_unix: burn_time,
                            dv_x: best_dv_x,
                            dv_y: best_dv_y,
                            dv_z: best_dv_z,
                        });
                        
                        app.debris_avoided += 1;
                    }
                }                
            }
        }
    }

    if !emergency_burns.is_empty() {
        app.engine.maneuver_queue.extend(emergency_burns);
    }
}
