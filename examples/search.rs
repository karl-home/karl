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