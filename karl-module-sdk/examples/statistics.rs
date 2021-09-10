//! Send data to statistics.com.
//!
//! Inputs
//! - data: some data in bytes
//! Outputs
//! Network access required
use karl_module_sdk::KarlModuleSDK;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = KarlModuleSDK::new();
    let data = api.get_event("data").await?.unwrap();
    api.network(
        "https://www.statistics.com",
        "GET",
        vec![],
        data,
    ).await?;
    Ok(())
}