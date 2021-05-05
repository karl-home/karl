#[macro_use]
extern crate log;
use log::LevelFilter;

use std::error::Error;
use std::time::{Instant, Duration};

use tokio;
use clap::{Arg, App};
use karl_module_sdk::KarlSensorSDK;

/// Register the sensor with the controller.
///
/// * keys
///     - on: whether the light bulb should be on or off
///
/// Returns: the sensor ID.
async fn register(
    api: &mut KarlSensorSDK,
) -> Result<String, Box<dyn Error>> {
    let now = Instant::now();
    let result = api.register(
        "bulb",
        vec![String::from("on")], // keys
        vec![], // tags
        vec![], // app
    ).await?;
    info!("registered sensor => {} s", now.elapsed().as_secs_f32());
    info!("sensor_token = {:?}", result.sensor_token);
    info!("sensor_id = {:?}", result.sensor_id);
    Ok(result.sensor_id)
}

async fn handle_state_changes(
    api: KarlSensorSDK,
) -> Result<(), Box<dyn Error>> {
    let mut conn = api.connect_state().await?;
    while let Some(msg) = conn.message().await? {
        if msg.key == "on" {
            let state = msg.value;
            if state.len() != 1 {
                warn!("invalid length message: {:?}", state);
                continue;
            }
            match state[0] {
                0 => info!("turning OFF"),
                1 => info!("turning ON"),
                byte => warn!("invalid byte: {}", byte),
            }
        } else {
            warn!("unexpected key: {}", msg.key);
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::builder().filter_level(LevelFilter::Info).init();
    let matches = App::new("Smart light bulb that turns on and off.")
        .arg(Arg::with_name("ip")
            .help("Controller ip.")
            .takes_value(true)
            .default_value("127.0.0.1"))
        .arg(Arg::with_name("port")
            .help("Controller port.")
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
    info!("Hello, I'm a light bulb! I can turn on and off.");
    let state_change_handle = {
        let api = api.clone();
        tokio::spawn(async move {
            const SLEEP_INTERVAL: u64 = 10;
            let duration = Duration::from_secs(SLEEP_INTERVAL);
            loop {
                match handle_state_changes(api.clone()).await {
                    Ok(()) => { break; },
                    Err(e) => {
                        error!(
                            "Connection ended. Try again in {}s",
                            SLEEP_INTERVAL,
                        );
                        error!("{}", e);
                    }
                }
                tokio::time::sleep(duration).await;
            }
        })
    };
    state_change_handle.await?;
    Ok(())
}
