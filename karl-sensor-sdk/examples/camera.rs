#[macro_use]
extern crate log;

use std::fs;
use std::error::Error;
use std::time::{Instant, Duration};
use clap::{Arg, App};
use tokio;
use karl_module_sdk::KarlSensorSDK;

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
    api: &mut KarlSensorSDK,
) -> Result<String, Box<dyn Error>> {
    let now = Instant::now();
    let result = api.register(
        "camera",
        vec![String::from("firmware"), String::from("livestream")], // keys
        vec![String::from("motion"), String::from("livestream")], // tags
        vec![], // app
    ).await?;
    debug!("registered sensor => {} s", now.elapsed().as_secs_f32());
    debug!("sensor_token = {:?}", result.sensor_token);
    debug!("sensor_id = {:?}", result.sensor_id);
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
            if msg.value == "on".as_bytes() {
                info!("turning livestream on");
                // TODO: send livestream messages
                // tokio::spawn(async move {
                // let tag = "streaming".to_string();
                // api.push(tag, image_bytes.clone()).await.unwrap();
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
    let mut interval = tokio::time::interval(duration);
    tokio::time::sleep(Duration::from_secs(10)).await;
    loop {
        let tag = "motion".to_string();
        interval.tick().await;
        warn!("START step 1: pushing image");
        let now = Instant::now();
        api.push(tag, image_bytes.clone()).await.unwrap();
        warn!("=> {} s", now.elapsed().as_secs_f32());
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::builder().init();
    let matches = App::new("Camera sensor")
        .arg(Arg::with_name("ip")
            .help("Controller ip.")
            .takes_value(true)
            .default_value("127.0.0.1"))
        .arg(Arg::with_name("port")
            .help("Controller port.")
            .takes_value(true)
            .default_value("59582"))
        .arg(Arg::with_name("interval")
            .help("Motion detection interval, in seconds.")
            .takes_value(true)
            .default_value("30"))
        .arg(Arg::with_name("image_path")
            .help("Path to the image to send when motion is detected.")
            .takes_value(true)
            .default_value("data/FudanPed00001_smaller.png"))
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
