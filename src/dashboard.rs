//! Controller dashboard.
use rocket;
use tokio::runtime::Runtime;

#[get("/")]
fn hello() -> &'static str {
    "Hello, world!"
}

pub fn start(rt: &mut Runtime) {
    rt.spawn(async move {
        rocket::ignite().mount("/", routes![hello]).launch();
    });
}