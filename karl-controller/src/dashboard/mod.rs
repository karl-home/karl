//! Controller dashboard.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use rocket_contrib::serve::StaticFiles;
use karl_common::{SensorToken, Client};
use crate::controller::HostScheduler;

mod endpoint;

pub fn start(
    hosts: Arc<Mutex<HostScheduler>>,
    sensors: Arc<Mutex<HashMap<SensorToken, Client>>>,
) {
    tokio::spawn(async move {
        rocket::ignite()
        .manage(hosts)
        .manage(sensors)
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
