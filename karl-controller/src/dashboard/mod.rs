//! Controller dashboard.
use std::sync::{Arc, Mutex};
use rocket_contrib::serve::StaticFiles;
use crate::controller::Controller;

mod endpoint;
mod graph;

pub fn start(controller: Controller) {
    let hosts = controller.scheduler.clone();
    let controller = Arc::new(Mutex::new(controller));
    tokio::spawn(async move {
        rocket::ignite()
        .manage(hosts)
        .manage(controller)
        .mount("/", StaticFiles::from("../karl-ui/dist"))
        .mount("/", routes![
            endpoint::get_graph,
            // endpoint::save_graph,
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
