#[macro_use]
extern crate log;

use std::error::Error;
use clap::{Arg, App};

async fn register_hooks(controller: &str) -> Result<(), Box<dyn Error>> {
    karl::net::register_hook(controller, "person_detection").await?;
    karl::net::register_hook(controller, "differential_privacy").await?;
    Ok(())
}

async fn _add_edges() -> Result<(), Box<dyn Error>> {
    // TODO: data edge camera.motion -> person_detection
    // TODO: data edge person_detection.count -> differential_privacy
    // TODO: network edge differential_privacy -> https://metrics.karl.zapto.org
    Ok(())
}

async fn _persist_tags() -> Result<(), Box<dyn Error>> {
    // TODO: persist person_detection.box_count
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	env_logger::builder().format_timestamp(None).init();
    let matches = App::new("Figure 4a setup")
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
    register_hooks(&addr).await?;
    Ok(())
}