import requests
from skyfield.api import load, EarthSatellite
from datetime import datetime, timezone
import time

# --- Configuration ---
API_ENDPOINT = "http://0.0.0.0:8000/api/telemetry"
MAX_SATELLITES = 50   # To match our hackathon spec
MAX_DEBRIS = 1000     # To keep the frontend rendering smoothly for this specific test

print("Initializing CelesTrak Feeder...")

# 1. Load the Skyfield timescale and current time
ts = load.timescale()
now_dt = datetime.now(timezone.utc)
t = ts.from_datetime(now_dt)

# 2. Fetch or Load Local TLEs
print("Checking for Starlink TLEs (will download if missing)...")
starlinks = load.tle_file(
    'https://celestrak.org/NORAD/elements/supplemental/sup-gp.php?FILE=starlink&FORMAT=tle',
    filename='starlink_tles.txt',
    reload=False # # <--- This tells Skyfield to use the local file if it exists
)

print("Checking for Cosmos-2251 (Debris) TLEs (will download if missing)...")
debris_cloud = load.tle_file(
    'https://celestrak.org/NORAD/elements/gp.php?GROUP=cosmos-2251-debris&FORMAT=tle',
    filename='cosmos_2251_tles.txt',
    reload=False
)

print(f"Loaded {len(starlinks)} Starlinks and {len(debris_cloud)} Debris objects.")

# 3. Process into the JSON Payload expected by the Rust Backend
objects_payload = []

# Process Satellites
for sat in starlinks[:MAX_SATELLITES]:
    # Compute the ECI position and velocity for 'now'
    geocentric = sat.at(t)
    # Skyfield returns AU and AU/day, convert to km and km/s
    pos_km = geocentric.position.km
    vel_kms = geocentric.velocity.km_per_s
    
    # Format the ID to match your API parser (e.g., SAT-Alpha-01)
    norad_id = sat.model.satnum
    formatted_id = f"SAT-Alpha-{norad_id}"

    objects_payload.append({
        "id": formatted_id,
        "type": "SATELLITE",
        "r": {"x": pos_km[0], "y": pos_km[1], "z": pos_km[2]},
        "v": {"x": vel_kms[0], "y": vel_kms[1], "z": vel_kms[2]}
    })

# Process Debris
for deb in debris_cloud[:MAX_DEBRIS]:
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

# 4. Construct the Final JSON
payload = {
    # Generate ISO 8601 string for the Rust parser
    "timestamp": now_dt.strftime("%Y-%m-%dT%H:%M:%S.000Z"),
    "objects": objects_payload
}

# 5. Fire it at the Rust Backend!
print(f"Injecting {len(objects_payload)} total objects into the physics engine...")

try:
    response = requests.post(API_ENDPOINT, json=payload)
    if response.status_code == 200:
        res_data = response.json()
        print("SUCCESS! Backend Response:")
        print(f"  Processed: {res_data.get('processed_count')}")
        print(f"  Active CDM Warnings: {res_data.get('active_cdm_warnings')}")
    else:
        print(f"Failed with status {response.status_code}: {response.text}")
except Exception as e:
    print(f"Connection Error: Ensure your Rust server is running on {API_ENDPOINT}")
