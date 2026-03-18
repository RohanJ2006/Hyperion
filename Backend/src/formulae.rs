use crate::constants::{J2_PERTURBATION, RADIUS_OF_EARTH, STANDARD_GRAVITATIONAL_PARAMETER};

// Struct for the JSON data we receive
pub struct StateVector {  
    // Position vectors (x, y, z)
    pub x: Vec<f64>, 
    pub y: Vec<f64>,
    pub z: Vec<f64>,

    // Velocity vectors (Vx, Vy, Vz)
    pub v_x: Vec<f64>, 
    pub v_y: Vec<f64>, 
    pub v_z: Vec<f64>,

    pub apogee: Vec<f64>,
    pub perigee: Vec<f64>, 
}

// Implementation of methods for StateVector
impl StateVector {
    // Pass self for capacity
    pub fn new(capacity: usize) -> Self { 
        Self {
            x: Vec::with_capacity(capacity), 
            y: Vec::with_capacity(capacity),
            z: Vec::with_capacity(capacity),

            v_x: Vec::with_capacity(capacity),
            v_y: Vec::with_capacity(capacity),
            v_z: Vec::with_capacity(capacity),

            apogee: Vec::with_capacity(capacity),
            perigee: Vec::with_capacity(capacity),
        }
    }

    // Push the object states(values) from JSON data
    pub fn push_states(&mut self, x: f64, y: f64, z: f64, v_x: f64, v_y: f64, v_z: f64) {
        self.x.push(x); 
        self.y.push(y);
        self.z.push(z);

        self.v_x.push(v_x);
        self.v_y.push(v_y);
        self.v_z.push(v_z);

        // Push 0.0 because we need to compute it
        self.apogee.push(0.0);
        self.perigee.push(0.0);
    }

    // Compute values for apogee and perigee
    pub fn compute_apogee_perigee(&mut self) { 
        let total_objects = self.x.len();
        
        // Use assert! macro to make LLVM to remove bound checks so if one check goes x.len() is equal to all these, it will never check for length again saving CPU time
        assert!(
            self.y.len() == total_objects && 
            self.z.len() == total_objects && 
            self.v_x.len() == total_objects && 
            self.v_y.len() == total_objects && 
            self.v_z.len() == total_objects &&
            self.apogee.len() == total_objects &&
            self.perigee.len() == total_objects
        );

        // Loop through all objects
        for i in 0..total_objects {
            // Position vector
            let x = self.x[i];
            let y = self.y[i];
            let z = self.z[i];

            // Velocity vector
            let v_x = self.v_x[i];
            let v_y = self.v_y[i];
            let v_z = self.v_z[i];

            // Square of position vector
            let r_square = x*x + y*y + z*z;

            // Magnitude of position vector
            let mag_r = r_square.sqrt();

            // Square of velocity vector
            let v_square = v_x * v_x + v_y * v_y + v_z * v_z;

            // Specific orbital energy being the total mechanical energy of the object
            let specific_orbital_energy = 0.5 * v_square - STANDARD_GRAVITATIONAL_PARAMETER / mag_r;

            // If divide by 0 error, they simply wont collide
            if specific_orbital_energy >= 0.0 {
                self.perigee[i] = f64::MAX;
                self.apogee[i] = f64::MAX;
                continue;
            }

            // Semi major axis is half of the longest diameter of the ellipse
            let semi_major_axis = - STANDARD_GRAVITATIONAL_PARAMETER / (2.0 * specific_orbital_energy);
            
            // Angular momentum
            let h_x = y * v_z - z * v_y;
            let h_y = z * v_x - x * v_z;
            let h_z = x * v_y - y * v_x;

            // Square of angular momentum
            let h_square = h_x * h_x + h_y * h_y + h_z * h_z;

            // Eccentricity is how much the object deviates from its path
            let eccentricity = (1.0 + (2.0 * specific_orbital_energy * h_square) / (STANDARD_GRAVITATIONAL_PARAMETER * STANDARD_GRAVITATIONAL_PARAMETER)).sqrt();

            // Computed the value of perigee
            self.perigee[i] = semi_major_axis * (1.0 - eccentricity);
            // Computed the value of apogee
            self.apogee[i] = semi_major_axis * (1.0 + eccentricity);
        }
    }
}
