//! Sends a randomized answer given the input data to metrics.com
//!
//! Inputs
//! - count
//! Network access required
use rand::Rng;
use karl_module_sdk::KarlModuleSDK;

const F:f32 = 0.5;
const Q:f32 = 0.75;
const P:f32 = 0.5;

struct UserRAPPOR {
  actual_response: u8,
  memoized_response: u8,
  f: f32,
  q: f32,
  p: f32,
}

impl UserRAPPOR {
  fn new(actual_response: u8, f: f32, q: f32, p: f32) -> Self {
    let mut rng = rand::thread_rng();
    let rand = rng.gen_range(0.0, 1.0);
    let mem_rep;
    if rand < f * 0.5 {
      mem_rep = 1;
    } 
    else if rand >= f * 0.5 && rand < f {
      mem_rep = 0;
    }
    else {
      mem_rep = actual_response;
    }
    return Self {actual_response, memoized_response:mem_rep, f, q, p};
  }
  fn get_response(&self) -> u8 {
    let result;
    let mut rng = rand::thread_rng();
    if self.memoized_response == 1 {
      if rng.gen_range(0.0, 1.0) < self.q {
        result = 1;
      } 
      else {
        result = 0;
      }
    } 
    else {
      if rng.gen_range(0.0, 1.0) < self.p { 
        result = 1;
      } 
      else {
        result = 0;
      }
    }
    result
  }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = KarlModuleSDK::new();
    //dummy value for testing
    let input = 1;
    let diff_priv = UserRAPPOR::new(input, F, Q, P);
    let output = vec![diff_priv.get_response()];
    let res = api.network("metrics.com", "POST", vec![], output).await;
    println!("response = {:?}", res);
    Ok(())
}
