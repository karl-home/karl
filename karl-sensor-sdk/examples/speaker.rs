#[macro_use]
extern crate log;
use log::LevelFilter;

use std::fs;
use std::error::Error;
use std::time::{Instant, Duration};

use tokio;
use clap::{Arg, App};
use karl_sensor_sdk::KarlSensorSDK;

/// Register the sensor with the controller.
///
/// * keys
///     - playback: an audio response to play back to the user
/// * tags
///     - speech_command: non-silence audio recorded from the user
///
/// Returns: the sensor ID.
async fn register(
    api: &mut KarlSensorSDK,
) -> Result<String, Box<dyn Error>> {
    let now = Instant::now();
    let result = api.register(
        "speaker",
        vec![String::from("playback")], // keys
        vec![String::from("speech_command")], // tags
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
        if msg.key == "playback" {
            match std::str::from_utf8(&msg.value[..]) {
                Ok(string) => {
                    info!("Playing: {}", string);
                },
                Err(e) => error!("{}", e),
            }
        } else {
            warn!("unexpected key: {}", msg.key);
        }
    }
    Ok(())
}

/// Push data at a regular interval to the speaker.speech_command tag
/// to represent when non-silence audio is detected.
async fn audio_detection(
    api: KarlSensorSDK,
    interval: u64,
    audio_path: String,
) -> Result<(), Box<dyn Error>> {
    let audio_bytes = fs::read(audio_path)?;
    let duration = Duration::from_secs(interval);
    let mut interval = tokio::time::interval(duration);
    tokio::time::sleep(Duration::from_secs(5)).await;
    loop {
        let tag = "speech_command".to_string();
        interval.tick().await;
        info!("pushing {} bytes of audio", audio_bytes.len());
        warn!("START SpeechLight: {:?}", std::time::Instant::now());
        api.push(tag, audio_bytes.clone()).await.unwrap();
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::builder().filter_level(LevelFilter::Info).init();
    let audio_path = "data/picovoice-coffee.wav";
    let audio_path = std::env::var("KARL_PATH")
        .map(|path| format!("{}/karl-sensor-sdk/{}", path, audio_path))
        .unwrap_or(audio_path.to_string());
    let matches = App::new("Smart light bulb that turns on and off.")
        .arg(Arg::with_name("ip")
            .help("Controller ip.")
            .takes_value(true)
            .long("ip")
            .default_value("127.0.0.1"))
        .arg(Arg::with_name("port")
            .help("Controller port.")
            .takes_value(true)
            .long("port")
            .default_value("59582"))
        .arg(Arg::with_name("interval")
            .help("Audio detection interval, in seconds.")
            .takes_value(true)
            .long("interval")
            .default_value("10"))
        .arg(Arg::with_name("audio_path")
            .help("Path to the audio to send when audio is detected.")
            .takes_value(true)
            .long("audio_path")
            .default_value(&audio_path))
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
    let audio_detection_handle = {
        let api = api.clone();
        let interval: u64 =
            matches.value_of("interval").unwrap().parse().unwrap();
        let audio_path = matches.value_of("audio_path").unwrap().to_string();
        tokio::spawn(async move {
            audio_detection(api, interval, audio_path).await.unwrap()
        })
    };
    state_change_handle.await?;
    audio_detection_handle.await?;
    Ok(())
}
