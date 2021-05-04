//! Outputs
//! - firmware: the firmware binary
//! Network access required
use karl_module_sdk::KarlModuleSDK;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = KarlModuleSDK::new();
    let res = api.network("firmware.com", "GET", vec![], vec![]).await;
    println!("response = {:?}", res);
    // Assume there was an update.
    let res = api.push("firmware", vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]).await;
    println!("response = {:?}", res);
    Ok(())
}