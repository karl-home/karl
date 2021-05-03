use serde_json;
use karl::net::KarlAPI;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tag = std::env::var("TRIGGERED_TAG")?;
    let timestamp = std::env::var("TRIGGERED_TIMESTAMP")?;
    let api = KarlAPI::new();
    let data = api.get(&tag, &timestamp, &timestamp).await?;
    let slots = serde_json::from_slice(&data[..])?;
    println!("slots = {:?}", slots);
    match slots {
        serde_json::Value::Object(map) => {
            if let Some(state) = map.get("state") {
                match state {
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
        }
        _ => {},
    }
    Ok(())
}