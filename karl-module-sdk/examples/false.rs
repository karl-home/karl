//! Outputs
//! - false: 0 bit
use karl_module_sdk::KarlModuleSDK;
#[tokio::main]
async fn main() {
    KarlModuleSDK::new().push("false", vec![0]).await.unwrap();
}
