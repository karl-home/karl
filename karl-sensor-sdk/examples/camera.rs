#[macro_use]
extern crate log;
use log::LevelFilter;

use std::fs;
use std::error::Error;
use std::time::{Instant, Duration};
use clap::{Arg, App};
use tokio;
use karl_sensor_sdk::KarlSensorSDK;

/// Register the sensor with the controller.
///
/// * keys
///     - firmware: new firmware to install
///     - livestream: whether the camera should be pushing a livestream
/// * returns
///     - motion: a snapshot image when motion is detected
///     - streaming: streaming write
///
/// Returns: the sensor token and sensor ID.
async fn register(
    api: &mut KarlSensorSDK,
) -> Result<String, Box<dyn Error>> {
    let now = Instant::now();
    let result = api.register(
        "camera",
        vec![String::from("firmware"), String::from("livestream")], // keys
        vec![String::from("motion"), String::from("streaming")], // returns
        vec![], // app
    ).await?;
    info!("registered sensor => {} s", now.elapsed().as_secs_f32());
    info!("sensor_token = {:?}", result.sensor_token);
    info!("sensor_id = {:?}", result.sensor_id);
    Ok(result.sensor_id)
}

/// Listen for state changes.
async fn handle_state_changes(
    api: KarlSensorSDK,
) -> Result<(), Box<dyn Error>> {
    let mut conn = api.connect_state().await?;
    while let Some(msg) = conn.message().await? {
        if msg.key == "firmware" {
            info!("firmware update!");
        } else if msg.key == "livestream" {
            warn!("FINISH LivestreamOn: {:?}", Instant::now());
            warn!("FINISH (chrono) LivestreamOn: {:?}", chrono::Utc::now().time());
            if msg.value == vec![1] {
                info!("turning livestream on");
                // TODO: send livestream messages
                // tokio::spawn(async move {
                // let output = "streaming".to_string();
                // api.push(output, image_bytes.clone()).await.unwrap();
            } else {
                info!("turning livestream off");
            }
        } else {
            warn!("unexpected key: {}", msg.key);
        }
    }
    Ok(())
}

/// Push data at a regular interval to the camera.motion tag
/// to represent snaphots of when motion is detected.
async fn motion_detection(
    api: KarlSensorSDK,
    interval: u64,
    image_path: String,
) -> Result<(), Box<dyn Error>> {
    let image_bytes = fs::read(image_path)?;
    let duration = Duration::from_secs(interval);
    tokio::time::sleep(Duration::from_secs(10)).await;
    let mut interval = tokio::time::interval(duration);
    loop {
        let output = "motion".to_string();
        interval.tick().await;
        warn!("START PersonDet: {:?}", std::time::Instant::now());
        warn!("START (chrono) PersonDet: {:?}", chrono::Utc::now().time());
        info!("pushing {} byte image", image_bytes.len());
        // let now = Instant::now();
        api.push(output, image_bytes.clone()).await.unwrap();
        // warn!("=> {} s", now.elapsed().as_secs_f32());
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::builder().filter_level(LevelFilter::Info).init();
    let image_path = "data/FudanPed00001_smaller.png";
    let image_path = std::env::var("KARL_PATH")
        .map(|path| format!("{}/karl-sensor-sdk/{}", path, image_path))
        .unwrap_or(image_path.to_string());
    let matches = App::new("Camera sensor")
        .arg(Arg::with_name("ip")
            .help("Controller ip.")
            .long("ip")
            .takes_value(true)
            .default_value("127.0.0.1"))
        .arg(Arg::with_name("port")
            .help("Controller port.")
            .long("port")
            .takes_value(true)
            .default_value("59582"))
        .arg(Arg::with_name("interval")
            .help("Motion detection interval, in seconds.")
            .long("interval")
            .takes_value(true)
            .default_value("30"))
        .arg(Arg::with_name("image_path")
            .help("Path to the image to send when motion is detected.")
            .long("image_path")
            .takes_value(true)
            .default_value(&image_path))
        .get_matches();

    let api = {
        let ip = matches.value_of("ip").unwrap();
        let port = matches.value_of("port").unwrap();
        let addr = format!("http://{}:{}", ip, port);
        let mut api = KarlSensorSDK::new(&addr);
        let _sensor_id = register(&mut api).await?;
        api
    };
    let state_change_handle = {
        let api = api.clone();
        tokio::spawn(async move {
            handle_state_changes(api).await.unwrap()
        })
    };
    let motion_detection_handle = {
        let api = api.clone();
        let interval: u64 =
            matches.value_of("interval").unwrap().parse().unwrap();
        let image_path = matches.value_of("image_path").unwrap().to_string();
        tokio::spawn(async move {
            motion_detection(api, interval, image_path).await.unwrap()
        })
    };
    state_change_handle.await?;
    motion_detection_handle.await?;
    Ok(())
}
