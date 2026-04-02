import requests
from datetime import datetime, timezone

BASE_URL = "http://localhost:8000"

print("Initiating Autonomous System Test...")

# 1. Inject an imminent collision scenario
now_dt = datetime.now(timezone.utc)
telemetry_payload = {
    "timestamp": now_dt.strftime("%Y-%m-%dT%H:%M:%S.000Z"),
    "objects": [
        {
            "id": "SAT-Alpha-5432",
            "type": "SATELLITE",
            "r": {"x": 7000.0, "y": 0.0, "z": 0.0},
            "v": {"x": 0.0, "y": 7.5, "z": 0.0}
        },
        {
            "id": "DEB-4321",
            "type": "DEBRIS",
            # Positioned 50 meters away, converging on the Z-axis
            "r": {"x": 7000.05, "y": 0.0, "z": 0.0},
            "v": {"x": 0.0, "y": 7.5, "z": 0.01} 
        }
    ]
}

print("\n1. Injecting collision vectors via Telemetry API...")
res_telemetry = requests.post(f"{BASE_URL}/api/telemetry", json=telemetry_payload)
print(f"Telemetry API Status: {res_telemetry.status_code}")

# 2. Advance the simulation to trigger the evasion brain
# We step forward by 15 seconds to evaluate the state. 
# Because last_screening_time initialized at 0.0, this will immediately bypass the throttle.
step_payload = {"step_seconds": 15}

print("\n2. Advancing simulation clock by 15 seconds...")
res_step = requests.post(f"{BASE_URL}/api/simulate/step", json=step_payload)
print(f"Simulation API Status: {res_step.status_code}")

print("\nTest payload delivered.")
print("Verify the autonomous execution in your Rust backend terminal output.")
