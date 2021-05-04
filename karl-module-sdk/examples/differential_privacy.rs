//! Sends a randomized answer given the input data to metrics.com
//!
//! Inputs
//! - count
//! Network access required
use karl_module_sdk::KarlModuleSDK;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = KarlModuleSDK::new();
    let input = api.get_triggered().await?.unwrap();
    let output = if fastrand::bool() {
        println!("counted {:?}", input);
        vec![input[0]]
    } else {
        vec![input[fastrand::usize(..input.len())]] // TODO
    };
    let res = api.network("metrics.com", "POST", vec![], output).await;
    println!("response = {:?}", res);
    Ok(())
}
