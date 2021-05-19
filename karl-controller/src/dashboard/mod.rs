//! Controller dashboard.
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::mpsc;
use rocket_contrib::serve::StaticFiles;
use karl_common::*;
use crate::controller::{QueuedHook, HostScheduler};

mod endpoint;

pub fn start(
    hosts: Arc<Mutex<HostScheduler>>,
    sensors: Arc<Mutex<HashMap<SensorToken, Client>>>,
    modules: Arc<Mutex<HashMap<HookID, Hook>>>,
    watched_tags: Arc<RwLock<HashMap<String, Vec<HookID>>>>,
    hook_tx: mpsc::Sender<QueuedHook>,
) {
    tokio::spawn(async move {
        rocket::ignite()
        .manage(hosts)
        .manage(sensors)
        .manage(modules)
        .manage(watched_tags)
        .manage(hook_tx)
        .mount("/", StaticFiles::from("../karl-ui/dist"))
        .mount("/", routes![
            endpoint::get_graph,
            endpoint::save_graph,
            endpoint::spawn_module,
            endpoint::confirm_sensor,
            endpoint::cancel_sensor,
            endpoint::get_sensors,
            endpoint::confirm_host,
            endpoint::cancel_host,
            endpoint::get_hosts,
        ])
        .launch();
    });
}
