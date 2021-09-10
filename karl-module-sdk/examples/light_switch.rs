//! Inputs
//! - light_intent: { state: on } or { state: off }
//! Outputs
//! - state: [1] for on and [0] for off
use serde_json;
use karl_module_sdk::KarlModuleSDK;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = KarlModuleSDK::new();
    let data = api.get_event("light_intent").await?.unwrap();
    let slots = serde_json::from_slice(&data[..])?;
    match slots {
        serde_json::Value::Object(map) => {
            match map.get("state").unwrap() {
                serde_json::Value::String(state) => {
                    if state == &"on".to_string() {
                        api.push("state", vec![1]).await?;
                    } else if state == &"off".to_string() {
                        api.push("state", vec![0]).await?;
                    }
                },
                _ => {},
            }
        }
        _ => {},
    }
    Ok(())
}