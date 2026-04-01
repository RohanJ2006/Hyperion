import requests
import time
from datetime import datetime, timezone, timedelta

BASE_URL = "http://localhost:8000"

print("Initiating Final Maneuver Stress Test...")

# 1. Inject an imminent collision
now_dt = datetime.now(timezone.utc)
collision_payload = {
    "timestamp": now_dt.strftime("%Y-%m-%dT%H:%M:%S.000Z"),
    "objects": [
        {
            "id": "SAT-Alpha-99",
            "type": "SATELLITE",
            "r": {"x": 7000.0, "y": 0.0, "z": 0.0},
            "v": {"x": 0.0, "y": 7.5, "z": 0.0}
        },
        {
            "id": "DEB-ASSASSIN-X",
            "type": "DEBRIS",
            "r": {"x": 7000.05, "y": 0.0, "z": 0.0},
            "v": {"x": 0.0, "y": 7.5, "z": 0.01} 
        }
    ]
}

print("\nInjecting Collision Course Data...")
res = requests.post(f"{BASE_URL}/api/telemetry", json=collision_payload)
print(f"Status: {res.status_code}")

print("\nStepping Simulation to Trigger Detection...")
res = requests.post(f"{BASE_URL}/api/simulate/step", json={"step_seconds": 10})
try:
    print(f"Status: {res.status_code} | Active Warnings in Engine: {res.json().get('collisions_detected')}")
except:
    print(f"Status: {res.status_code} | Response: {res.text}")

# 3. Schedule the Evasive Maneuver
burn_time = now_dt + timedelta(seconds=15)

maneuver_payload = {
    "satelliteId": "SAT-Alpha-99",
    "maneuver_sequence": [
        {
            "burn_id": "EVASION-BURN-01",
            "burnTime": burn_time.strftime("%Y-%m-%dT%H:%M:%S.000Z"), 
            "deltaV_vector": {"x": 0.01, "y": 0.0, "z": 0.0}         
        }
    ]
}

print("\nSending Evasive Maneuver Command...")
res = requests.post(f"{BASE_URL}/api/maneuver/schedule", json=maneuver_payload)
print(f"Status: {res.status_code}")

if res.status_code in [200, 202]:
    print(f"Response: {res.json()}")
    print("\nSUCCESS! The satellite accepted the burn and is dodging the debris!")
else:
    # If it fails, print the raw text so we can read the exact Rust error
    print(f"Response: {res.text}")
    print("\nFAILED! The satellite rejected the maneuver.")
