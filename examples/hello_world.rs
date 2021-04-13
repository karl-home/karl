#[macro_use]
extern crate log;

use std::fs;
use std::time::{Instant, Duration};
use clap::{Arg, App};
use tokio;
use karl;

/// Requests computation from the host.
///
/// Params:
/// - controller: <IP>:<PORT>
async fn send(controller: &str) -> Result<(), Box<dyn std::error::Error>> {
    let now = Instant::now();
    let result = karl::net::register_sensor(controller, "hello-world-client", vec![]).await?.into_inner();
    debug!("registered sensor => {} s", now.elapsed().as_secs_f32());
    debug!("sensor_token = {:?}", result.sensor_token);
    debug!("sensor_id = {:?}", result.sensor_id);

    let now = Instant::now();
    karl::net::register_hook(controller, &result.sensor_token, "hello-world").await?;
    debug!("registered hook => {} s", now.elapsed().as_secs_f32());

    let image_path = "data/person-detection/PennFudanPed/PNGImages/FudanPed00001.png";
    let image_bytes = fs::read(image_path)?;
    let controller = controller.to_string();
    let join_handle = tokio::spawn(async move {
        let duration = Duration::from_secs(10);
        let mut interval = tokio::time::interval(duration);
        loop {
            interval.tick().await;
            karl::net::push_raw_data(
                &controller,
                result.sensor_token.clone(),
                image_bytes.clone(),
            ).await.unwrap();
        }
    });
    join_handle.await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::builder().format_timestamp(None).init();
    let matches = App::new("Speech-to-text")
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
    send(&addr).await?;
    info!("done.");
    Ok(())
}
