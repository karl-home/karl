//! Outputs
//! - true: 1 bit
use karl_module_sdk::KarlModuleSDK;
#[tokio::main]
async fn main() {
    KarlModuleSDK::new().push("true", vec![1]).await.unwrap();
}
