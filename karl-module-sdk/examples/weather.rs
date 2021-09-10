//! Quer.
//!
//! Inputs
//! - weather_intent: an intent expressing the query { location: text }
//! Outputs
//! - response: the text response
//! Network access required
use serde_json;
use karl_module_sdk::KarlModuleSDK;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = KarlModuleSDK::new();
    let data = api.get_event("weather_intent").await?.unwrap();
    let slots = serde_json::from_slice(&data[..])?;
    println!("slots = {:?}", slots);
    match slots {
        serde_json::Value::Object(map) => {
            if let Some(location) = map.get("location") {
                match location {
                    serde_json::Value::String(location) => {
                        let res = api.network("https://www.weather.com", "GET", vec![], location.as_bytes().to_vec()).await;
                        println!("response = {:?}", res);
                        api.push("weather", b"the weather is great!".to_vec()).await?;
                    },
                    _ => {},
                }
            }
        }
        _ => {},
    }
    Ok(())
}