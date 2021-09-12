#[macro_use]
extern crate log;
use log::LevelFilter;

use std::error::Error;
use std::time::Instant;
use clap::{Arg, App};
use tokio;
use karl_sensor_sdk::KarlSensorSDK;

/// Register the sensor with the controller.
///
/// * keys
///     - state: whether the light is on or off
///     - intensity: the intensity of the light from 1-10
/// * returns
///     - state: whether the light is on or off
///     - intensity: the intensity of the light from 1-10
///
/// Returns: the sensor token and sensor ID.
async fn register(
    api: &mut KarlSensorSDK,
) -> Result<String, Box<dyn Error>> {
    let now = Instant::now();
    let result = api.register(
        "light",
        vec![String::from("state"), String::from("intensity")],
        vec![String::from("state"), String::from("intensity")],
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
        if msg.key == "state" {
            let state = msg.value;
            if state.len() != 1 {
                warn!("invalid length message: {:?}", state);
                continue;
            }
            warn!("FINISH SpeechLight: {:?}", std::time::Instant::now());
            match state[0] {
                0 => info!("turning OFF"),
                1 => info!("turning ON"),
                byte => warn!("invalid byte: {}", byte),
            }
            api.push("state".to_string(), state).await.unwrap();
        } else if msg.key == "intensity" {
            let intensity = msg.value;
            if intensity.len() != 1 {
                warn!("invalid length message: {:?}", intensity);
                continue;
            }
            info!("turning light to intensity {}", intensity[0]);
            api.push("intensity".to_string(), intensity).await.unwrap();
        } else {
            warn!("unexpected key: {}", msg.key);
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::builder().filter_level(LevelFilter::Info).init();
    let matches = App::new("Lightb bulb")
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
    state_change_handle.await?;
    Ok(())
}
