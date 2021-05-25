//! Controller dashboard.
mod client;
mod helper;
mod cookie;

use std::fs;
use std::io::Write;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;

use tokio;
use rocket::{self, http::Cookies, State};
use rocket_contrib::templates::Template;
use serde::Serialize;

use karl_common::{SensorToken, Host, Client};
use crate::controller::HostScheduler;
use helper::*;
use cookie::SessionState;

#[derive(Serialize)]
struct MainContext {
    title: &'static str,
    base_domain: String,
    hosts: Vec<Host>,
    sensors: Vec<Client>,
}

/// Returns the client app if the page is a subdomain of the base domain,
/// otherwise returns the main controller dashboard.
///
/// The main dashboard visualizes the hosts and sensors, and provides an
/// interface for the user to manually verify hosts and sensors. Provides
/// links to client apps. Basic HTTP authentication with NGINX. Client
/// subdomains are granted a cookie with an expiration date to access
/// proxy/ and storage/ resources behind that subdomain. Those resources
/// require both the cookie and are behind a strict CORS policy.
#[get("/")]
fn index(
    host_header: HostHeader,
    base_domain: State<String>,
    karl_path: State<PathBuf>,
    cookies: Cookies,
    hosts: State<Arc<Mutex<HostScheduler>>>,
    sensors: State<Arc<Mutex<HashMap<SensorToken, Client>>>>,
    sessions: State<Arc<Mutex<SessionState>>>,
) -> Option<Template> {
    match to_client_id(&host_header, base_domain.to_string()) {
        Some(client_id) => client::index(client_id, karl_path, cookies, sessions),
        None => Some(Template::render("index", &MainContext {
            title: "Hello",
            base_domain: base_domain.to_string(),
            hosts: hosts.lock().unwrap().hosts(),
            sensors: sensors.lock().unwrap().values().map(
                |client| client.clone()).collect(),
        })),
    }
}

pub fn start(
    karl_path: PathBuf,
) {
    let base_domain = "karl.zapto.org".to_string();
    let sessions = Arc::new(Mutex::new(SessionState::new()));
    tokio::spawn(async move {
        rocket::ignite()
        .manage(karl_path)
        .manage(base_domain)
        .manage(sessions)
        .mount("/", routes![index])
        .mount("/", routes![client::storage, client::proxy_get])
        .attach(Template::custom(|engines| {
            engines.handlebars.register_helper("request", Box::new(request_helper));
            engines.handlebars.register_helper("heartbeat", Box::new(heartbeat_helper));
        }))
        .launch();
    });
}
