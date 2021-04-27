#[macro_use]
extern crate log;

use std::error::Error;
use clap::{Arg, App};

/// Registers a hook with the global hook ID and returns the hook ID.
async fn register_hook(
    controller: &str,
    global_hook_id: &str,
) -> Result<String, Box<dyn Error>> {
    let res = karl::net::register_hook(controller, global_hook_id).await?;
    let hook_id = res.into_inner().hook_id;
    info!("register hook {} => {}", global_hook_id, hook_id);
    Ok(hook_id)
}

/// Adds edges
async fn add_edges(
    controller: &str,
    hook_id: String,
) -> Result<(), Box<dyn Error>> {
    // network edge firmware_update -> https://firmware.karl.zapto.org
    let domain = "https://firmware.karl.zapto.org";
    info!("add network edge from {} to {}", hook_id, domain);
    karl::net::add_network_edge(
        controller,
        hook_id.clone(),
        String::from(domain),
    ).await?;

    // state edge firmware_update.firmware -> camera.firmware
    info!("add state edge from {}.firmware to camera.firmware", hook_id);
    karl::net::add_state_edge(
        controller,
        hook_id,
        String::from("firmware"),
        String::from("camera"),
        String::from("firmware"),
    ).await?;
    Ok(())
}

async fn set_schedule(
    controller: &str,
    hook_id: String,
) -> Result<(), Box<dyn Error>> {
    let seconds = 5;
    info!("set schedule every {}s", seconds);
    karl::net::set_interval(
        controller,
        hook_id,
        seconds,
    ).await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::builder().format_timestamp(None).init();
    let matches = App::new("Figure 4c setup")
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
    let firmware_hook_id = register_hook(&addr, "firmware_update").await.unwrap();
    add_edges(&addr, firmware_hook_id.clone()).await.unwrap();
    set_schedule(&addr, firmware_hook_id).await.unwrap();
    Ok(())
}
