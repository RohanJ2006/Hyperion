const ENTITY_STRIDE: usize = 6;
const MAX_ENTITIES: usize = 25_000;
const BUFFER_LEN: usize = MAX_ENTITIES * ENTITY_STRIDE;

static mut MEMORY_BUFFER: [f64; BUFFER_LEN] = [0.0_f64; BUFFER_LEN];

#[unsafe(no_mangle)]
pub extern "C" fn get_memory_pointer() -> *mut f64 {
  std::ptr::addr_of_mut!(MEMORY_BUFFER).cast::<f64>()
}

#[unsafe(no_mangle)]
pub extern "C" fn get_buffer_len() -> usize {
  BUFFER_LEN
}

#[unsafe(no_mangle)]
pub extern "C" fn compute_mercator(count:usize , width:f64 , height:f64){
  let n: usize = if count > MAX_ENTITIES {MAX_ENTITIES} else {count};

  const MAX_LATITUDE: f64 = 85.051129;

  //constants outside the loop for faster calculations as multiplication is faster than division 
  let deg_to_rad: f64 = std::f64::consts::PI / 180.0;
  let pi_over_4: f64 = std::f64::consts::PI / 4.0;
  let inv_pi: f64 = 1.0 / std::f64::consts::PI;
  let x_scale: f64 = width / 360.0;
  let x_offset: f64 = 180.0 * x_scale;

  let half_height:f64 = height / 2.0;

  let buff: &mut [f64]= unsafe {
    &mut MEMORY_BUFFER[0..(n* ENTITY_STRIDE)]
  };

  for entity in buff.chunks_exact_mut(ENTITY_STRIDE) {
      let lat_deg: f64 = entity[1];
      let lng_deg: f64 = entity[2];

      //mercator calculation from latitude and longitude 
      let x:f64 = (lng_deg * x_scale) + x_offset;

      let clamped_lat: f64 = lat_deg.clamp(-MAX_LATITUDE, MAX_LATITUDE);
      let lat_rad: f64 = clamped_lat * deg_to_rad;
      let merc_n: f64 = (pi_over_4 + lat_rad * 0.5).tan().ln();
      let y: f64 = half_height * (1.0 - merc_n * inv_pi);

      entity[4] = x;
      entity[5] = y;
    }
}