use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::path::Path;
use std::io::Write;
use chrono::Utc;

#[derive(Serialize)]
pub struct ManeuverLog {
    pub log_timestamp: String,
    pub event_type: String,
    pub satellite_id: u32,
    pub threat_id: Option<u32>,
    pub pca_km: Option<f64>,
    pub dv_x: f64,
    pub dv_y: f64,
    pub dv_z: f64,
    pub execution_time: f64,
}

/// Appends a structured JSON log to the logs/mission_audit.jsonl file.
pub fn record_maneuver(log: ManeuverLog) {
    if let Ok(json_line) = serde_json::to_string(&log) {
        let log_dir = "logs";
        
        // Autonomously create the logs directory if it does not exist
        if !Path::new(log_dir).exists() {
            if fs::create_dir(log_dir).is_err() {
                eprintln!("[CRITICAL] Failed to create logs directory.");
                return;
            }
        }

        let file_path = format!("{}/mission_audit.jsonl", log_dir);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path);

        if let Ok(mut f) = file {
            let _ = writeln!(f, "{}", json_line);
        } else {
            eprintln!("[CRITICAL] Failed to open audit file for writing.");
        }
    }
}
