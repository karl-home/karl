#[macro_use]
extern crate log;

use std::time::Instant;
use clap::{Arg, App};
use karl;

/// Requests computation from the host.
///
/// Params:
/// - controller: <IP>:<PORT>
async fn send(controller: &str) -> Result<(), Box<dyn std::error::Error>> {
    debug!("registering sensor");
    let now = Instant::now();
    let result = karl::net::register_sensor(controller, "hello-world-client", vec![]).await?.into_inner();
    info!("sensor_token = {:?}", result.sensor_token);
    info!("sensor_id = {:?}", result.sensor_id);
    debug!("=> {} s", now.elapsed().as_secs_f32());

    debug!("registering hook");
    let now = Instant::now();
    karl::net::register_hook(controller, &result.sensor_token, "hello-world").await?;
    debug!("=> {} s", now.elapsed().as_secs_f32());
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
