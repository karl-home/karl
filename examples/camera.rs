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
/// Returns: the sensor token and sensor ID.
async fn register(
    controller: &str,
) -> Result<(String, String), Box<dyn Error>> {
    let now = Instant::now();
    let result = karl::net::register_sensor(
        controller,
        "camera",
        vec![String::from("firmware"), String::from("livestream")], // keys
        vec![String::from("motion"), String::from("livestream")], // tags
        vec![], // app
    ).await?.into_inner();
    debug!("registered sensor => {} s", now.elapsed().as_secs_f32());
    debug!("sensor_token = {:?}", result.sensor_token);
    debug!("sensor_id = {:?}", result.sensor_id);
    Ok((result.sensor_token, result.sensor_id))
}

/// Listen for state changes.
async fn handle_state_changes(
    controller: String,
    sensor_token: String,
) -> Result<(), Box<dyn Error>> {
    let keys = vec![String::from("firmware"), String::from("livestream")];
    let mut conn = karl::net::connect_state(
        &controller, &sensor_token, keys.clone()).await?.into_inner();
    while let Some(msg) = conn.message().await? {
        if msg.key == keys[0] {
            println!("firmware update!");
        } else if msg.key == keys[1] {
            // TODO: on livestream, start writing to livestream tag
        } else {
            println!("unexpected key: {}", msg.key);
        }
    }
    Ok(())
}

/// Push data at a regular interval to the camera.motion tag
/// to represent snaphots of when motion is detected.
async fn motion_detection(
    controller: String,
    sensor_token: String,
) -> Result<(), Box<dyn Error>> {
    let image_path = "data/person-detection/PennFudanPed/PNGImages/FudanPed00001.png";
    let image_bytes = fs::read(image_path)?;
    let duration = Duration::from_secs(30);
    let mut interval = tokio::time::interval(duration);
    tokio::time::sleep(Duration::from_secs(10)).await;
    loop {
        let tag = "motion".to_string();
        interval.tick().await;
        karl::net::push_raw_data(
            &controller,
            sensor_token.clone(),
            tag,
            image_bytes.clone(),
        ).await.unwrap();
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
    let (sensor_token, sensor_id) = register(&addr).await?;
    let state_change_handle = {
        let addr = addr.to_string();
        let sensor_token = sensor_token.clone();
        tokio::spawn(async move {
            handle_state_changes(addr, sensor_token).await.unwrap()
        })
    };
    let motion_detection_handle = {
        let addr = addr.to_string();
        let sensor_token = sensor_token.clone();
        let _sensor_id = sensor_id.clone();
        tokio::spawn(async move {
            motion_detection(addr, sensor_token).await.unwrap()
        })
    };
    state_change_handle.await?;
    motion_detection_handle.await?;
    Ok(())
}
