#[macro_use]
extern crate log;

use std::collections::HashMap;
use std::error::Error;
use clap::{Arg, App};

const GLOBAL_HOOK_IDS: [&'static str; 9] = [
    "command_classifier",
    "search",
    "light_switch",
    "firmware_update",
    "person_detection",
    "differential_privacy",
    "targz",
    "true",
    "false",
];

const SENSOR_IDS: [&'static str; 4] = [
    "mic",
    "mic_1",
    "bulb",
    "camera",
];

/// Parses a string <entity_id>.<tag> as (<entity_id>, <tag>)
fn parse_tag(tag: &str) -> (&str, &str) {
    let mut split = tag.split(".");
    (split.next().unwrap(), split.next().unwrap())
}

/// Registers a hook with the global hook ID and returns the hook ID.
async fn register_hook(
    addr: &str,
    global_hook_id: &str,
) -> Result<String, Box<dyn Error>> {
    let res = karl::net::register_hook(addr, global_hook_id).await?;
    let hook_id = res.into_inner().hook_id;
    info!("register hook {} => {}", global_hook_id, hook_id);
    Ok(hook_id)
}

/// Register all hooks for this test.
/// Returns a map from global_hook_id to registered_hook_id.
async fn register_hooks(
    addr: &str,
    global_hook_ids: &[&str],
) -> HashMap<String, String> {
    let mut hook_ids = HashMap::new();
    for global_hook_id in global_hook_ids {
        match register_hook(&addr, global_hook_id).await {
            Ok(hook_id) => {
                hook_ids.insert(global_hook_id.to_string(), hook_id);
            },
            Err(error) => error!("{}", error),
        }
    }
    hook_ids
}

/// Generate the graph from Figures 4 and 6 based on registered hooks.
async fn generate_graph(hook_ids: HashMap<String, String>) {
    let data_edges_stateless = vec![
        ("mic.sound", "command_classifier"),
        ("mic_1.sound", "command_classifier"),
        ("command_classifier.search", "search"),
        ("command_classifier.light", "light_switch"),
        ("camera.motion", "person_detection"),
        ("person_detection.count", "differential_privacy"),
    ];
    let data_edges_stateful = vec![
        ("camera.streaming", "targz"),
    ];
    let state_edges = vec![
        ("google.response", "mic.output"),
        ("google.response", "mic_1.output"),
        ("light_switch.state", "bulb.on"),
        ("true.true", "camera.livestream"),
        ("false.false", "camera.livestream"),
        ("firmware_update.firmware", "camera.firmware"),
    ];
    let network_edges = vec![
        ("search", "google.com"),
        ("differential_privacy", "metrics.com"),
        ("firmware_update", "firmware.com"),
    ];
    let interval_schedule = vec![
        ("firmware_update", 10),
    ];
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::builder().format_timestamp(None).init();
    let matches = App::new("Figure 4a setup")
        .arg(Arg::with_name("ip")
            .help("Controller ip.")
            .takes_value(true)
            .default_value("127.0.0.1"))
        .arg(Arg::with_name("port")
            .help("Controller port.")
            .takes_value(true)
            .default_value("59582"))
        .get_matches();

    let ip = matches.value_of("ip").unwrap();
    let port = matches.value_of("port").unwrap();
    let addr = format!("http://{}:{}", ip, port);
    let hook_ids = register_hooks(&addr, &GLOBAL_HOOK_IDS).await;
    let graph = generate_graph(hook_ids).await;
    Ok(())
}