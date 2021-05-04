//! Search google given a text query from a classifier.
//!
//! Inputs
//! - query_intent: an intent expressing the query { query: text }
//! Outputs
//! - response: the text response
//! Network access required
use serde_json;
use karl_module_sdk::KarlModuleSDK;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = KarlModuleSDK::new();
    let data = api.get_triggered().await?.unwrap();
    let slots = serde_json::from_slice(&data[..])?;
    println!("slots = {:?}", slots);
    match slots {
        serde_json::Value::Object(map) => {
            if let Some(query) = map.get("query") {
                match query {
                    serde_json::Value::String(query) => {
                        let res = api.network("https://www.google.com", "GET", vec![], query.as_bytes().to_vec()).await;
                        println!("response = {:?}", res);
                        api.push("response", b"the weather is great!".to_vec()).await?;
                    },
                    _ => {},
                }
            }
        }
        _ => {},
    }
    Ok(())
}