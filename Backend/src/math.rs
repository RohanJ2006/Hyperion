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
