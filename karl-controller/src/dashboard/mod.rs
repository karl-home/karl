//! Controller dashboard.
use rocket_contrib::serve::StaticFiles;

mod endpoint;

pub fn start() {
    tokio::spawn(async move {
        rocket::ignite()
        .mount("/", StaticFiles::from("dist"))
        .launch();
    });
}
