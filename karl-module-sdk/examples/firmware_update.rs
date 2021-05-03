use std::error::Error;
use karl::net::KarlAPI;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let api = KarlAPI::new();
    let res = api.network("https://firmware.karl.zapto.org", "GET", vec![], vec![]).await;
    println!("response = {:?}", res);
    // Assume there was an update.
    let res = api.push("firmware", vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]).await;
    println!("response = {:?}", res);
    Ok(())
}