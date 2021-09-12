#[macro_use]
extern crate log;
use log::LevelFilter;

use std::error::Error;
use std::time::{Instant, Duration};
use clap::{Arg, App};
use tokio;
use karl_sensor_sdk::KarlSensorSDK;

/// Register the sensor with the controller.
///
/// * keys
/// * returns
///     - at_home: 0- or 1-bit for whether the user is at home.
///
/// Returns: the sensor token and sensor ID.
async fn register(
    api: &mut KarlSensorSDK,
) -> Result<String, Box<dyn Error>> {
    let now = Instant::now();
    let result = api.register(
        "occupancy_sensor",
        vec![],
        vec![String::from("at_home")],
        vec![], // app
    ).await?;
    info!("registered sensor => {} s", now.elapsed().as_secs_f32());
    info!("sensor_token = {:?}", result.sensor_token);
    info!("sensor_id = {:?}", result.sensor_id);
    Ok(result.sensor_id)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::builder().filter_level(LevelFilter::Info).init();
    let matches = App::new("Occupancy sensor")
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
        .arg(Arg::with_name("at_home")
            .help("If the flag is present, the user is not at home.")
            .long("not_at_home"))
        .get_matches();

    let api = {
        let ip = matches.value_of("ip").unwrap();
        let port = matches.value_of("port").unwrap();
        let addr = format!("http://{}:{}", ip, port);
        let mut api = KarlSensorSDK::new(&addr);
        let _sensor_id = register(&mut api).await?;
        api
    };
    // Sleep so it has time to register an edge.
    tokio::time::sleep(Duration::from_secs(10)).await;
    let at_home = if matches.is_present("not_at_home") {
        vec![0]
    } else {
        vec![1]
    };
    info!("at home? {}", at_home[0]);
    api.push(String::from("at_home"), at_home).await.unwrap();
    Ok(())
}
