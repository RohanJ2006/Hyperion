// EARTH AND PHYSICS CONSTANTS

// Standard gravitational parameter of the earth in (km^3/s^2)
pub const STANDARD_GRAVITATIONAL_PARAMETER: f64 = 398600.4418;

// Radius of the earth in (km)
pub const RADIUS_OF_EARTH: f64 = 6378.137;

// J2 perturbation coefficient of earth (no unit)
pub const J2_PERTURBATION: f64 = 1.08263e-3;

// Acceleration due to standard gravity in (m/s^2)
pub const STANDARD_GRAVITY_M: f64 = 9.80665;

// Acceleration due to standard gravity in (km/s^2)
pub const STANDARD_GRAVITY_KM: f64 = 0.00980665;

// SATELLITE CONSTANTS

// Dry mass of the satellite without fuel in (kg)
pub const DRY_MASS: f64 = 500.0;

// Initial propellant fuel in (kg)
pub const INITIAL_PROPELLANT_MASS: f64 = 50.0;

// Initial mass of satellite and fuel together in (kg)
pub const INITIAL_WET_MASS: f64 = 550.0;

// Specific impulse of the satellite in (s)
pub const SPECIFIC_IMPULSE: f64 = 300.0;

// Maximum velocity change per burn in (km/s)
pub const MAX_THRUST_DELTA: f64 = 0.015;

// Fuel threshold below which satellite is sent to graveyard orbit in (kg)
pub const EOL_FUEL_THRESHOLD: f64 = 2.5;

// OPERATIONAL THRESHOLDS

// Distance below which satellite should perform a maneuver in (km)
pub const CRITICAL_CONJUNCTION_DISTANCE: f64 = 0.100;

// Spherical radius in which the satellite can drift in (km)
pub const DRIFT_TOLERANCE: f64 = 10.0;

// Mandatory cooldown between two burst commands in (s)
pub const THRUSTER_COOLDOWN: u64 = 600;

// Hardcoded delay for API commands in (s)
pub const COMMUNICATION_LATENCY: u64 = 10;

// Conjunction prediction should be done over this window in (s)
pub const PREDICTION_WINDOW: u64 = 86400;

// The API port to be exposed
pub const API_PORT: u16 = 8000;
