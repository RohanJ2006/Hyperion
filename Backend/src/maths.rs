use std::f64::consts::PI;
use crate::constants::*;

// Calculates the magnitude (length) of a 3D Vector
#[inline(always)]
pub fn magnitude(v: (f64, f64, f64)) -> f64 {
    (v.0 * v.0 + v.1 * v.1 + v.2 * v.2).sqrt()
}

// Calculates the cross product of two vectors
#[inline(always)]
pub fn cross_product(a: (f64, f64, f64), b: (f64, f64, f64)) -> (f64, f64, f64) {
    (
        a.1 * b.2 - a.2 * b.1,
        a.2 * b.0 - a.0 * b.2,
        a.0 * b.1 - a.1 * b.0,
    )
}

// Calculates the dot product of two vectors
#[inline(always)]
pub fn dot_product(a: (f64, f64, f64), b: (f64, f64, f64)) -> f64 {
    a.0 * b.0 + a.1 * b.1 + a.2 * b.2
}

// Get base RTN unit vectors for a given ECI position and velocity
#[inline(always)]
pub fn get_rtn_base(
    position: (f64, f64, f64),
    velocity: (f64, f64, f64),
) -> ((f64, f64, f64), (f64, f64, f64), (f64, f64, f64)) {
    let radial_magnitude = magnitude(position);
    // Guard: avoid division by zero on malformed telemetry
    if radial_magnitude < 1e-10 {
        return ((1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0));
    }
    let radial_unit_vector = (
        position.0 / radial_magnitude,
        position.1 / radial_magnitude,
        position.2 / radial_magnitude,
    );

    let angular_momentum = cross_product(position, velocity);
    let angular_magnitude = magnitude(angular_momentum);
    if angular_magnitude < 1e-10 {
        return ((1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0));
    }
    let normal_unit_vector = (
        angular_momentum.0 / angular_magnitude,
        angular_momentum.1 / angular_magnitude,
        angular_momentum.2 / angular_magnitude,
    );

    // T = N × R
    let transverse_unit_vector = cross_product(normal_unit_vector, radial_unit_vector);

    (radial_unit_vector, transverse_unit_vector, normal_unit_vector)
}

// Projects an arbitrary ECI vector into the local RTN frame
pub fn eci_to_rtn(
    position: (f64, f64, f64),
    velocity: (f64, f64, f64),
    vector: (f64, f64, f64),
) -> (f64, f64, f64) {
    let (r, t, n) = get_rtn_base(position, velocity);
    (dot_product(vector, r), dot_product(vector, t), dot_product(vector, n))
}

// Converts a delta-v vector from RTN frame into ECI frame
pub fn rtn_to_eci(
    position: (f64, f64, f64),
    velocity: (f64, f64, f64),
    delta_v: (f64, f64, f64),
) -> (f64, f64, f64) {
    let (r, t, n) = get_rtn_base(position, velocity);
    (
        r.0 * delta_v.0 + t.0 * delta_v.1 + n.0 * delta_v.2,
        r.1 * delta_v.0 + t.1 * delta_v.1 + n.1 * delta_v.2,
        r.2 * delta_v.0 + t.2 * delta_v.1 + n.2 * delta_v.2,
    )
}

// Computes the J2-perturbed acceleration vector for a given ECI position (km, km/s²)
pub fn j2_acceleration(x: f64, y: f64, z: f64) -> (f64, f64, f64) {
    let r2 = x * x + y * y + z * z;
    let r = r2.sqrt();
    let r3 = r2 * r;
    let r5 = r3 * r2;

    let gravity_coefficient = STANDARD_GRAVITATIONAL_PARAMETER / r3;
    let z_ratio = (z * z) / r2;
    let j2_coefficient =
        1.5 * J2_PERTURBATION * STANDARD_GRAVITATIONAL_PARAMETER * (RADIUS_OF_EARTH * RADIUS_OF_EARTH) / r5;

    let ax = -gravity_coefficient * x + j2_coefficient * x * (5.0 * z_ratio - 1.0);
    let ay = -gravity_coefficient * y + j2_coefficient * y * (5.0 * z_ratio - 1.0);
    let az = -gravity_coefficient * z + j2_coefficient * z * (5.0 * z_ratio - 3.0);

    (ax, ay, az)
}

// Pure two-body acceleration (no J2). Used for propagating nominal orbital slots.
#[inline(always)]
pub fn two_body_acceleration(x: f64, y: f64, z: f64) -> (f64, f64, f64) {
    let r3 = (x * x + y * y + z * z).powf(1.5);
    let c = -STANDARD_GRAVITATIONAL_PARAMETER / r3;
    (c * x, c * y, c * z)
}

// Single RK4 step for one object with J2 perturbation.
// Input:  position (km), velocity (km/s), time step (s)
// Output: (new_x, new_y, new_z, new_vx, new_vy, new_vz)
#[inline(always)]
pub fn rk4_step(
    x: f64, y: f64, z: f64,
    vx: f64, vy: f64, vz: f64,
    dt: f64,
) -> (f64, f64, f64, f64, f64, f64) {
    let dt2 = dt / 2.0;
    let dt6 = dt / 6.0;

    // k1 — slope at start
    let a1 = j2_acceleration(x, y, z);

    // k2 — slope at midpoint using k1
    let r2 = (x + vx * dt2, y + vy * dt2, z + vz * dt2);
    let v2 = (vx + a1.0 * dt2, vy + a1.1 * dt2, vz + a1.2 * dt2);
    let a2 = j2_acceleration(r2.0, r2.1, r2.2);

    // k3 — slope at midpoint using k2
    let r3 = (x + v2.0 * dt2, y + v2.1 * dt2, z + v2.2 * dt2);
    let v3 = (vx + a2.0 * dt2, vy + a2.1 * dt2, vz + a2.2 * dt2);
    let a3 = j2_acceleration(r3.0, r3.1, r3.2);

    // k4 — slope at end using k3
    let r4 = (x + v3.0 * dt, y + v3.1 * dt, z + v3.2 * dt);
    let v4 = (vx + a3.0 * dt, vy + a3.1 * dt, vz + a3.2 * dt);
    let a4 = j2_acceleration(r4.0, r4.1, r4.2);

    (
        x  + dt6 * (vx + 2.0 * v2.0 + 2.0 * v3.0 + v4.0),
        y  + dt6 * (vy + 2.0 * v2.1 + 2.0 * v3.1 + v4.1),
        z  + dt6 * (vz + 2.0 * v2.2 + 2.0 * v3.2 + v4.2),
        vx + dt6 * (a1.0 + 2.0 * a2.0 + 2.0 * a3.0 + a4.0),
        vy + dt6 * (a1.1 + 2.0 * a2.1 + 2.0 * a3.1 + a4.1),
        vz + dt6 * (a1.2 + 2.0 * a2.2 + 2.0 * a3.2 + a4.2),
    )
}

// Single RK4 step using pure two-body gravity (for nominal slot propagation).
#[inline(always)]
pub fn two_body_step(
    x: f64, y: f64, z: f64,
    vx: f64, vy: f64, vz: f64,
    dt: f64,
) -> (f64, f64, f64, f64, f64, f64) {
    let dt2 = dt / 2.0;
    let dt6 = dt / 6.0;

    let a1 = two_body_acceleration(x, y, z);

    let r2 = (x + vx * dt2, y + vy * dt2, z + vz * dt2);
    let v2 = (vx + a1.0 * dt2, vy + a1.1 * dt2, vz + a1.2 * dt2);
    let a2 = two_body_acceleration(r2.0, r2.1, r2.2);

    let r3 = (x + v2.0 * dt2, y + v2.1 * dt2, z + v2.2 * dt2);
    let v3 = (vx + a2.0 * dt2, vy + a2.1 * dt2, vz + a2.2 * dt2);
    let a3 = two_body_acceleration(r3.0, r3.1, r3.2);

    let r4 = (x + v3.0 * dt, y + v3.1 * dt, z + v3.2 * dt);
    let v4 = (vx + a3.0 * dt, vy + a3.1 * dt, vz + a3.2 * dt);
    let a4 = two_body_acceleration(r4.0, r4.1, r4.2);

    (
        x  + dt6 * (vx + 2.0 * v2.0 + 2.0 * v3.0 + v4.0),
        y  + dt6 * (vy + 2.0 * v2.1 + 2.0 * v3.1 + v4.1),
        z  + dt6 * (vz + 2.0 * v2.2 + 2.0 * v3.2 + v4.2),
        vx + dt6 * (a1.0 + 2.0 * a2.0 + 2.0 * a3.0 + a4.0),
        vy + dt6 * (a1.1 + 2.0 * a2.1 + 2.0 * a3.1 + a4.1),
        vz + dt6 * (a1.2 + 2.0 * a2.2 + 2.0 * a3.2 + a4.2),
    )
}

// Propagate a single object by `dt` seconds using sub-stepped RK4 with J2.
// Sub-step size is 30s for accuracy on large time steps.
pub fn propagate_object(
    mut x: f64, mut y: f64, mut z: f64,
    mut vx: f64, mut vy: f64, mut vz: f64,
    dt: f64,
) -> (f64, f64, f64, f64, f64, f64) {
    if dt.abs() < 1e-9 { return (x, y, z, vx, vy, vz); }
    const SUB_STEP: f64 = 30.0;
    let n = ((dt.abs() / SUB_STEP).ceil() as usize).max(1);
    let actual = dt / n as f64;
    for _ in 0..n {
        let r = rk4_step(x, y, z, vx, vy, vz, actual);
        x = r.0; y = r.1; z = r.2;
        vx = r.3; vy = r.4; vz = r.5;
    }
    (x, y, z, vx, vy, vz)
}

// Calculates Greenwich Mean Sidereal Time (GMST) in radians from a Unix timestamp.
// Uses the IAU Earth Rotation Angle formulation.
#[inline(always)]
pub fn calculate_gmst(unix_timestamp: f64) -> f64 {
    // FIX: Correct J2000 epoch = Jan 1 2000, 11:58:55.816 UTC (= 12:00:00 TT)
    const J2000_UNIX_EPOCH_CORRECTED: f64 = 946727935.816;
    let days_since_j2000 = (unix_timestamp - J2000_UNIX_EPOCH_CORRECTED) / 86400.0;
    let mut gmst = 2.0 * PI * (EARTH_ROTATION_ANGLE + EARTH_ROTATION_RATE * days_since_j2000);
    gmst %= 2.0 * PI;
    if gmst < 0.0 { gmst += 2.0 * PI; }
    gmst
}

// Rotates an ECI position into ECEF by the GMST angle (rotation about Z-axis)
#[inline(always)]
pub fn eci_to_ecef(position: (f64, f64, f64), gmst: f64) -> (f64, f64, f64) {
    let (cos_g, sin_g) = (gmst.cos(), gmst.sin());
    (
        position.0 * cos_g + position.1 * sin_g,
        -position.0 * sin_g + position.1 * cos_g,
        position.2,
    )
}

// Iterative Bowring method to convert ECEF to geodetic (lat/lon/alt).
// Returns: (latitude_rad, longitude_rad, altitude_km)
pub fn ecef_to_geodetic(ecef: (f64, f64, f64)) -> (f64, f64, f64) {
    let (x, y, z) = ecef;
    let longitude = y.atan2(x);
    let p = (x * x + y * y).sqrt();

    // Parametric (reduced) latitude as seed — converges in 5 iterations
    let mut latitude = z.atan2(p * (1.0 - EARTH_ECCENTRICITY_SQUARED));
    let mut n = RADIUS_OF_EARTH; // prime vertical radius of curvature

    for _ in 0..5 {
        let sin_lat = latitude.sin();
        n = RADIUS_OF_EARTH / (1.0 - EARTH_ECCENTRICITY_SQUARED * sin_lat * sin_lat).sqrt();
        latitude = (z + n * EARTH_ECCENTRICITY_SQUARED * sin_lat).atan2(p);
    }

    // FIX: Use numerically stable altitude formula that avoids cos() singularity at poles
    let altitude = if latitude.abs() > 1.0 {
        // Near-polar: use z-component formula
        z / latitude.sin() - n * (1.0 - EARTH_ECCENTRICITY_SQUARED)
    } else {
        p / latitude.cos() - n
    };

    (latitude, longitude, altitude)
}

// Wrapper: ECI → Geodetic using the current simulation time for GMST
pub fn eci_to_geodetic(position: (f64, f64, f64), unix_timestamp: f64) -> (f64, f64, f64) {
    let gmst = calculate_gmst(unix_timestamp);
    let ecef = eci_to_ecef(position, gmst);
    ecef_to_geodetic(ecef)
}

// Converts geodetic (lat_rad, lon_rad, alt_km) to ECEF (km)
pub fn geodetic_to_ecef(latitude: f64, longitude: f64, altitude: f64) -> (f64, f64, f64) {
    let sin_lat = latitude.sin();
    let cos_lat = latitude.cos();
    let n = RADIUS_OF_EARTH / (1.0 - EARTH_ECCENTRICITY_SQUARED * sin_lat * sin_lat).sqrt();

    let x = (n + altitude) * cos_lat * longitude.cos();
    let y = (n + altitude) * cos_lat * longitude.sin();
    let z = (n * (1.0 - EARTH_ECCENTRICITY_SQUARED) + altitude) * sin_lat;
    (x, y, z)
}

// Calculates the elevation angle (degrees) of a satellite as seen from a ground station
pub fn calculate_elevation_angle(
    sat_ecef: (f64, f64, f64),
    gs_lat_rad: f64,
    gs_lon_rad: f64,
    gs_ecef: (f64, f64, f64),
) -> f64 {
    let slant = (
        sat_ecef.0 - gs_ecef.0,
        sat_ecef.1 - gs_ecef.1,
        sat_ecef.2 - gs_ecef.2,
    );
    // Spherical zenith unit vector at ground station location
    let zenith = (
        gs_lat_rad.cos() * gs_lon_rad.cos(),
        gs_lat_rad.cos() * gs_lon_rad.sin(),
        gs_lat_rad.sin(),
    );
    let dot = dot_product(slant, zenith);
    let slant_mag = magnitude(slant);
    if slant_mag < 1e-10 { return 90.0; }
    let zenith_angle = (dot / slant_mag).acos();
    (PI / 2.0 - zenith_angle).to_degrees()
}

// Computes propellant mass burned for a given delta-v using the Tsiolkovsky equation.
// dv is in km/s; returns burned mass in kg.
pub fn calculate_fuel_burn(current_mass: f64, dv_km_s: f64) -> f64 {
    let dv_m_s = dv_km_s * 1000.0;
    let exhaust_velocity = SPECIFIC_IMPULSE * STANDARD_GRAVITY_M;
    let mass_fraction = (-dv_m_s / exhaust_velocity).exp();
    current_mass * (1.0 - mass_fraction)
}

// This function is used by conjunction.rs file to replace the old propagate_linear
pub fn propagate_rk4_to(
    pos: (f64, f64, f64), vel: (f64, f64, f64), dt: f64,
) -> ((f64, f64, f64), (f64, f64, f64)) {
    let (x, y, z) = pos;
    let (vx, vy, vz) = vel;

    let (nx, ny, nz, nvx, nvy, nvz) = rk4_step(x, y, z, vx, vy, vz, dt);

    ((nx, ny, nz), (nvx, nvy, nvz))
}
