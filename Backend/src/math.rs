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

// Get base RTN values for ECI and RTN calculations
#[inline(always)]
pub fn get_rtn_base(
    position: (f64, f64, f64),
    velocity: (f64, f64, f64),
) -> ((f64, f64, f64), (f64, f64, f64), (f64, f64, f64)) {
    // Radial unit vector(R) - Points from earth center through the satellite
    let radial_magnitude = magnitude(position);
    let radial_unit_vector = (
        position.0 / radial_magnitude,
        position.1 / radial_magnitude,
        position.2 / radial_magnitude
    );

    // Normal unit vector(N) - Orthogonal to orbital plane (R x V)
    let angular_momentum = cross_product(position, velocity);
    let angular_magnitude = magnitude(angular_momentum);
    let normal_unit_vector = (
        angular_momentum.0 / angular_magnitude,
        angular_momentum.1 / angular_magnitude,
        angular_momentum.2 / angular_magnitude
    );

    // Transverse unit vector(T) - Points in the direction of velocity, perpendicular to R
    let transverse_unit_vector = cross_product(normal_unit_vector, radial_unit_vector);

    (radial_unit_vector, transverse_unit_vector, normal_unit_vector)
}

// Converts ECI frame to RTN
pub fn eci_to_rtn(
    position: (f64, f64, f64),
    velocity: (f64, f64, f64),
    target: (f64, f64, f64)
) -> (f64, f64, f64) {
    // Get base values from the function
    let (
        radial_unit_vector,
        transverse_unit_vector,
        normal_unit_vector
    ) = get_rtn_base(position, velocity);

    (
        dot_product(target, radial_unit_vector),
        dot_product(target, transverse_unit_vector),
        dot_product(target, normal_unit_vector)
    )
}

// Converts RTN frame to ECI
pub fn rtn_to_eci(
    position: (f64, f64, f64),
    velocity: (f64, f64, f64),
    delta_v: (f64, f64, f64)
) -> (f64, f64, f64) {
    // Get base values from function
    let (
        radial_unit_vector,
        transverse_unit_vector,
        normal_unit_vector
    ) = get_rtn_base(position, velocity);

    (
        radial_unit_vector.0 * delta_v.0 + transverse_unit_vector.0 * delta_v.1 + normal_unit_vector.0 * delta_v.2,
        radial_unit_vector.1 * delta_v.0 + transverse_unit_vector.1 * delta_v.1 + normal_unit_vector.1 * delta_v.2,
        radial_unit_vector.2 * delta_v.0 + transverse_unit_vector.2 * delta_v.1 + normal_unit_vector.2 * delta_v.2
    )
}


// Calculates Greenwich Mean Sidereal Time (GMST) in radians from Unix Timestamp
#[inline(always)]
pub fn calculate_gmst(unix_timestamp: f64) -> f64 {
    let days_since_j2000 = (unix_timestamp - J2000_UNIX_EPOCH) / 86400.0; // keeping everything f64 cause rust is strictly typed
    let mut gmst = 2.0 * PI * (EARTH_ROTATION_ANGLE + EARTH_ROTATION_RATE * days_since_j2000);
    
    // Normalize to 0 -> 2PI
    gmst %= 2.0 * PI;
    if gmst < 0.0 {
        gmst += 2.0 * PI;
    }
    gmst
}

// Converts Earth-Centered Inertial (ECI) to Earth-Centered, Earth-Fixed (ECEF)
#[inline(always)]
pub fn eci_to_ecef(position: (f64, f64, f64), gmst: f64) -> (f64, f64, f64) {
    let cos_gmst = gmst.cos();
    let sin_gmst = gmst.sin();

    // Rotate around the Z-axis by the GMST angle
    (
        position.0 * cos_gmst + position.1 * sin_gmst,
        -position.0 * sin_gmst + position.1 * cos_gmst,
        position.2 // Z remains unchanged (ignoring polar motion)
    )
}

// Converts ECEF to Geodetic (Latitude, Longitude, Altitude)
// Returns: (Latitude in radians, Longitude in radians, Altitude in km)
pub fn ecef_to_geodetic(ecef: (f64, f64, f64)) -> (f64, f64, f64) {
    let (x, y, z) = ecef;
    
    // Longitude is a straight forward calculation (no iterations required)
    let longitude = y.atan2(x);
    
    let p = (x * x + y * y).sqrt();
    
    // Initial guess for latitude assuming a spherical Earth (only seed)
    // This is the starting point of our Newton-Raphson method
    let mut latitude = z.atan2(p * (1.0 - EARTH_ECCENTRICITY_SQUARED));
    let mut n = 0.0;
    
    // 5 Iterations are enough as they provide millimeter of precision
    for _ in 0..5 {
        let sin_lat = latitude.sin();
        n = RADIUS_OF_EARTH / (1.0 - EARTH_ECCENTRICITY_SQUARED * sin_lat * sin_lat).sqrt();
        latitude = (z + n * EARTH_ECCENTRICITY_SQUARED * sin_lat).atan2(p);
    }
    
    // Final altitude calculation
    let altitude = p / latitude.cos() - n;
    
    (latitude, longitude, altitude) // latitude, longitude are returned in radians
}

// Wrapper function to go straight from ECI to Geodetic
// Here we get the unix_timestamp after converting the ISO-8601 from the telementry API
pub fn eci_to_geodetic(position: (f64, f64, f64), unix_timestamp: f64) -> (f64, f64, f64) {
    let gmst = calculate_gmst(unix_timestamp);
    let ecef = eci_to_ecef(position, gmst);
    ecef_to_geodetic(ecef)
}

// Computes the J2 acceleration for the satellie
pub fn j2_acceleration(x: f64, y: f64, z: f64) -> (f64, f64, f64) {
    let r2 = x * x + y * y + z * z;
    let r = r2.sqrt();
    let r3 = r2 * r;
    let r5 = r3 * r2;

    // Represents the gravitational pull of a perfect spherical object
    let gravity_coefficient = STANDARD_GRAVITATIONAL_PARAMETER / r3;

    // Square of the sine of the satellite's latitude (common part of the matrix in eq)
    let z_ratio = (z * z) / r2;

    // Coefficient of j2 which acts on the orbit of satellite
    let j2_coefficient = 1.5 * J2_PERTURBATION * STANDARD_GRAVITATIONAL_PARAMETER * (RADIUS_OF_EARTH * RADIUS_OF_EARTH) / r5;

    // The final 3D acceleration vector
    let ax = - gravity_coefficient * x + j2_coefficient * x * (5.0 * z_ratio - 1.0);
    let ay = - gravity_coefficient * y + j2_coefficient * y * (5.0 * z_ratio - 1.0);
    let az = - gravity_coefficient * z + j2_coefficient * z * (5.0 * z_ratio - 3.0);

    (ax, ay, az)
}
