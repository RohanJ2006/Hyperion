use crate::constants::{J2_PERTURBATION, RADIUS_OF_EARTH, STANDARD_GRAVITATIONAL_PARAMETER};
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct StateVector {
    pub position: Vector3<f64>, // Position vector for ECI in (km)
    pub velocity: Vector3<f64>, // Velocity vector for ECI in (km/s)
}

pub fn orbital_propagation(position_vector: &Vector3<f64>) -> Vector3<f64> {
    let position_norm = position_vector.norm(); // Magnitude of position vector
    let position_norm_squared = position_norm.powi(2); // Square of magnitude of position vector

    let keplerian_orbit = -(STANDARD_GRAVITATIONAL_PARAMETER / (position_norm_squared * position_norm)) * position_vector; // Two body value of keplerian orbit, the first part of the equation

    let j2_left_coefficient = 1.5 * J2_PERTURBATION * STANDARD_GRAVITATIONAL_PARAMETER * RADIUS_OF_EARTH.powi(2) / position_norm.powi(5); // The first part of the j2 acceleration vector equation which will substract kepler orbit
    
    let z_over_r = 5.0 * (position_vector.z.powi(2) / position_norm_squared); // Common value in the matrix 5z^2 / |r|^2

    let x_component = j2_left_coefficient * position_vector.x * (z_over_r - 1.0); // X component of the matrix
    let y_component = j2_left_coefficient * position_vector.y * (z_over_r - 1.0); // Y component of the matrix
    let z_component = j2_left_coefficient * position_vector.z * (z_over_r - 3.0); // Z component of the matrix

    let j2_acceleration_vector = Vector3::new(x_component, y_component, z_component); // The vector form of the x, y, z components to form the j2 acceleration vector

    keplerian_orbit + j2_acceleration_vector // Return the value of both by adding them, no need to explicitly add return keyword
}
