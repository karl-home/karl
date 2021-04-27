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

/// Adds edges:
/// data edge - sensor "camera" to module `pd_hook_id`
/// data edge - module `pd_hook_id` to module `dp_hook_id`
/// network edge - module `dp_hook_id` to domain
async fn add_edges(
    controller: &str,
    pd_hook_id: String,
    dp_hook_id: String,
) -> Result<(), Box<dyn Error>> {
    // data edge camera.motion -> person_detection
    info!("add data edge from camera.motion to {}", pd_hook_id);
    karl::net::add_data_edge(
        controller,
        String::from("camera"), // output_id
        String::from("motion"), // output_tag
        pd_hook_id.clone(), // input_id
        true, // trigger
    ).await?;

    // data edge person_detection.count -> differential_privacy
    info!("add data edge from {}.count to {}", pd_hook_id, dp_hook_id);
    karl::net::add_data_edge(
        controller,
        pd_hook_id.clone(),
        String::from("count"),
        dp_hook_id.clone(),
        true,
    ).await?;

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
    let pd_hook_id = register_hook(&addr, "person_detection").await?;
    let dp_hook_id = register_hook(&addr, "differential_privacy").await?;
    add_edges(&addr, pd_hook_id, dp_hook_id).await?;
    Ok(())
}