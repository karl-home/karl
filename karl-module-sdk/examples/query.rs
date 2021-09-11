//! Compress video from the past hour.
//!
//! Inputs
//! - image_data: image_data to compress
//! Outputs
//! - result: a single targz of those image_data
use tar::Builder;
use flate2::{Compression, write::GzEncoder};
use chrono::Duration;
use chrono::prelude::Local;
use karl_module_sdk::KarlModuleSDK;

#[tokio::main]
async fn main() {
    let api = KarlModuleSDK::new();
    let end = Local::now();
    let start = end.checked_sub_signed(Duration::hours(1)).unwrap();
    let res = api.get(
        "image_data",
        &start.format("%+").to_string(),
        &end.format("%+").to_string(),
    ).await.unwrap();
    println!("{} image_data", res.data.len());
    let mut buffer = Vec::new();
    let enc = GzEncoder::new(&mut buffer, Compression::default());
    let mut tar = Builder::new(enc);
    for (i, data) in res.data.into_iter().enumerate() {
        let timestamp = res.timestamps.get(i).unwrap();
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as _);
        tar.append_data(
            &mut header,
            timestamp,
            data.as_slice(),
        ).unwrap();
    }
    tar.finish().unwrap();
    drop(tar);
    println!("{} bytes", buffer.len());
    if !buffer.is_empty() {
        api.push("result", buffer).await.unwrap();
    }
}
