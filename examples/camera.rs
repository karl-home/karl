#[macro_use]
extern crate log;

use std::fs;
use std::error::Error;
use std::time::{Instant, Duration};
use clap::{Arg, App};
use tokio;
use karl;

/// Register the sensor with the controller.
///
/// * keys
///     - firmware: new firmware to install
///     - livestream: whether the camera should be pushing a livestream
/// * tags
///     - motion: a snapshot image when motion is detected
///     - livestream: streaming write
///
/// Returns: the sensor ID.
async fn register(controller: &str) -> Result<String, Box<dyn Error>> {
    let now = Instant::now();
    let result = karl::net::register_sensor(
        controller,
        "camera",
        vec![],
    ).await?.into_inner();
    debug!("registered sensor => {} s", now.elapsed().as_secs_f32());
    debug!("sensor_token = {:?}", result.sensor_token);
    debug!("sensor_id = {:?}", result.sensor_id);
    Ok(result.sensor_id)
}

/// Listen for state changes.
async fn handle_state_changes() -> Result<(), Box<dyn Error>> {
    // TODO: on firmware, "install" the firmware
    // TODO: on livestream, start writing to livestream tag
    Ok(())
}

/// Push data at a regular interval to the camera.motion tag
/// to represent snaphots of when motion is detected.
async fn motion_detection(
    _controller: String,
) -> Result<(), Box<dyn Error>> {
    let image_path = "data/person-detection/PennFudanPed/PNGImages/FudanPed00001.png";
    let _image_bytes = fs::read(image_path)?;
    let duration = Duration::from_secs(10);
    let mut interval = tokio::time::interval(duration);
    loop {
        interval.tick().await;
        println!("detected motion");
        // karl::net::push_raw_data(
        //     &controller,
        //     result.sensor_token.clone(),
        //     image_bytes.clone(),
        // ).await.unwrap();
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::builder().format_timestamp(None).init();
    let matches = App::new("Camera sensor")
        .arg(Arg::with_name("ip")
            .help("Controller ip.")
            .long("ip")
            .takes_value(true)
            .default_value("127.0.0.1"))
        .arg(Arg::with_name("port")
            .help("Controller port.")
            .short("p")
            .long("port")
            .takes_value(true)
            .default_value("59582"))
        .get_matches();

    let ip = matches.value_of("ip").unwrap();
    let port = matches.value_of("port").unwrap();
    let addr = format!("http://{}:{}", ip, port);
    let _sensor_id = register(&addr).await?;
    let state_change_handle = {
        tokio::spawn(async move { handle_state_changes().await.unwrap() })
    };
    let motion_detection_handle = {
        let addr = addr.to_string();
        tokio::spawn(async move { motion_detection(addr).await.unwrap() })
    };
    state_change_handle.await?;
    motion_detection_handle.await?;
    Ok(())
}
