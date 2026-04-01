import requests
from skyfield.api import load
from datetime import datetime, timezone
import time

# --- Configuration ---
API_ENDPOINT = "http://localhost:8000/api/telemetry"
STEP_ENDPOINT = "http://localhost:8000/api/simulate/step"

# THE FLIP: 50 Satellites, 12,000+ Debris
MAX_SATELLITES = 50
MAX_DEBRIS = 12000

print("Initializing CelesTrak Stress-Test Feeder...")

# 1. Load the Skyfield timescale and current time
ts = load.timescale()
now_dt = datetime.now(timezone.utc)
t = ts.from_datetime(now_dt)

# 2. Fetch or Load Local TLEs
print("Checking for Starlink TLEs (will download if missing)...")
starlinks = load.tle_file(
    'https://celestrak.org/NORAD/elements/supplemental/sup-gp.php?FILE=starlink&FORMAT=tle',
    filename='starlink_tles.txt', 
    reload=False
)

print("Checking for Cosmos-2251 TLEs (will download if missing)...")
debris_cloud = load.tle_file(
    'https://celestrak.org/NORAD/elements/gp.php?GROUP=cosmos-2251-debris&FORMAT=tle',
    filename='cosmos_2251_tles.txt',
    reload=False
)

print(f"Loaded {len(starlinks)} Starlinks and {len(debris_cloud)} Cosmos fragments.")
print("Crunching orbital math for 10,000+ objects... (This might take Python 5-10 seconds!)")

objects_payload = []

# 3. Process Cosmos-2251 fragments as our SATELLITES (Cap at 50)
for sat in debris_cloud[:MAX_SATELLITES]:
    geocentric = sat.at(t)
    pos_km = geocentric.position.km
    vel_kms = geocentric.velocity.km_per_s
    
    norad_id = sat.model.satnum
    formatted_id = f"SAT-Alpha-{norad_id}"

    objects_payload.append({
        "id": formatted_id,
        "type": "SATELLITE",
        "r": {"x": pos_km[0], "y": pos_km[1], "z": pos_km[2]},
        "v": {"x": vel_kms[0], "y": vel_kms[1], "z": vel_kms[2]}
    })

# 4. Process Starlinks as our DEBRIS CLOUD (Take all of them!)
for deb in starlinks[:MAX_DEBRIS]:
    geocentric = deb.at(t)
    pos_km = geocentric.position.km
    vel_kms = geocentric.velocity.km_per_s
    
    norad_id = deb.model.satnum
    formatted_id = f"DEB-{norad_id}"

    objects_payload.append({
        "id": formatted_id,
        "type": "DEBRIS",
        "r": {"x": pos_km[0], "y": pos_km[1], "z": pos_km[2]},
        "v": {"x": vel_kms[0], "y": vel_kms[1], "z": vel_kms[2]}
    })

# 5. Construct the Final JSON
payload = {
    "timestamp": now_dt.strftime("%Y-%m-%dT%H:%M:%S.000Z"),
    "objects": objects_payload
}

# 6. Fire it at the Rust Backend!
print(f"Injecting {len(objects_payload)} total objects into the Rust physics engine...")

try:
    # Send Telemetry
    response = requests.post(API_ENDPOINT, json=payload)
    if response.status_code == 200:
        print("Telemetry Ingested Successfully!")
        
        # 7. Auto-Trigger the Spatial Hash Grid!
        print("Fast-forwarding simulation by 60 seconds to trigger Conjunction Assessment...")
        step_payload = {"step_seconds": 60}
        step_res = requests.post(STEP_ENDPOINT, json=step_payload)
        
        if step_res.status_code == 200:
            print(f"Grid Search Complete! Check your Rust terminal for the benchmark logs.")
        else:
            print(f"Step Failed: {step_res.status_code} - {step_res.text}")

    else:
        print(f"Ingestion Failed with status {response.status_code}: {response.text}")
except Exception as e:
    print(f"Connection Error: Ensure your Rust server is running. Error: {e}")
