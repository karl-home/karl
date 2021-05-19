#[macro_use]
extern crate log;
use log::LevelFilter;
use std::collections::HashMap;
use std::error::Error;
use clap::{Arg, App};
use karl_user_sdk::{Graph, KarlUserSDK};

const GLOBAL_HOOK_IDS: [&'static str; 2] = [
    "person_detection",
    "differential_privacy",
];

const SENSOR_IDS: [&'static str; 1] = [
    "camera",
];

/// Register all hooks for this test.
/// Returns a map from global_hook_id to registered_hook_id.
async fn register_hooks(
    api: &KarlUserSDK,
    global_hook_ids: &[&str],
) -> HashMap<String, String> {
    let mut hook_ids = HashMap::new();
    for global_hook_id in global_hook_ids {
        match api.register_hook(global_hook_id).await {
            Ok(res) => {
                info!("registered {} as {}", global_hook_id, res.hook_id);
                hook_ids.insert(global_hook_id.to_string(), res.hook_id);
            },
            Err(error) => error!("{}", error),
        }
    }
    hook_ids
}

/// Generate the graph from Figures 4 and 6 based on registered hooks.
async fn generate_graph(hook_ids: HashMap<String, String>) -> Graph {
    let data_edges_stateless = vec![
        ("camera.motion", "person_detection.image"),
        ("person_detection.count", "differential_privacy.count"),
    ];
    let data_edges_stateful = vec![
    ];
    let state_edges = vec![
    ];
    let network_edges = vec![
        ("differential_privacy", "metrics.com"),
    ];
    let interval_modules = vec![
    ];
    let mut graph = Graph::new(
        SENSOR_IDS.to_vec(),
        GLOBAL_HOOK_IDS.to_vec(),
        data_edges_stateless,
        data_edges_stateful,
        state_edges,
        network_edges,
        interval_modules,
    );
    graph.replace_module_ids(&hook_ids);
    graph
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::builder().filter_level(LevelFilter::Info).init();
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
    let api = KarlUserSDK::new(&addr);
    let hook_ids = register_hooks(&api, &GLOBAL_HOOK_IDS).await;
    // let hook_ids = GLOBAL_HOOK_IDS
    //     .iter()
    //     .map(|hook_id| (hook_id.to_string(), hook_id.to_string()))
    //     .collect::<HashMap<String, String>>();
    let graph = generate_graph(hook_ids).await;
    println!("{}", graph.graphviz().unwrap());
    graph.send_to_controller(&api).await.unwrap();
    Ok(())
}