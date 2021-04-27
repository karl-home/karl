use std::env;
use std::error::Error;
use karl::net::KarlAPI;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let tag = env::var("TRIGGERED_TAG")?;
    let timestamp = env::var("TRIGGERED_TIMESTAMP")?;
    let api = KarlAPI::new();
    let data = api.get(&tag, &timestamp, &timestamp).await?;
    println!("counted {:?}", data);
    Ok(())
}
