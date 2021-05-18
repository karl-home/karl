//! Controller dashboard.
use rocket_contrib::serve::StaticFiles;

mod endpoint;

pub fn start() {
    tokio::spawn(async move {
        rocket::ignite()
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
