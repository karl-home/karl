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

use crate::common::SensorToken;
use crate::controller::{HostScheduler, types::{Host, Client}};
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

// TODO: authenticate this request. Only the homeowner can call this.
#[post("/confirm/host/<service_name>")]
fn confirm_host(
    host_header: HostHeader,
    base_domain: State<String>,
    service_name: String,
    hosts: State<Arc<Mutex<HostScheduler>>>,
) {
    if to_client_id(&host_header, base_domain.to_string()).is_some() {
        // Confirm must be to the base domain only.
        return;
    }
    hosts.lock().unwrap().confirm_host(&service_name);
}

// TODO: authenticate this request. Only the homeowner can call this.
/// User confirms the client with this name, indicating it is trusted.
///
/// The endpoint additionally writes the client to `<KARL_PATH>/clients.txt`
/// so the client will automatically re-register if the controller restarts.
#[post("/confirm/client/<client_id>")]
fn confirm_client(
    host_header: HostHeader,
    karl_path: State<PathBuf>,
    base_domain: State<String>,
    client_id: String,
    sensors: State<Arc<Mutex<HashMap<SensorToken, Client>>>>,
) {
    if to_client_id(&host_header, base_domain.to_string()).is_some() {
        // Confirm must be to the base domain only.
        return;
    }
    let mut sensors = sensors.lock().unwrap();
    for (token, client) in sensors.iter_mut() {
        if client.id != client_id {
            continue;
        }
        if client.confirmed {
            warn!("attempted to confirm already confirmed client: {:?}", client_id);
            return;
        } else {
            // Create the client file if it does not already exist.
            let path = karl_path.join("clients.txt");
            let mut file = fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&path)
                .expect(&format!("unable to open sensors file: {:?}", &path));
            let res = writeln!(file, "{}:{}={}", client.id, client.addr, token);
            if let Err(e) = res {
                error!("error serializing client {:?}: {:?}", client, e);
            }
            info!("confirmed client {:?}", client_id);
            client.confirmed = true;
            return;
        }
    }
    warn!("attempted to confirm nonexistent client: {:?}", client_id);
}

pub fn start(
    karl_path: PathBuf,
    hosts: Arc<Mutex<HostScheduler>>,
    sensors: Arc<Mutex<HashMap<SensorToken, Client>>>,
) {
    let base_domain = "karl.zapto.org".to_string();
    let sessions = Arc::new(Mutex::new(SessionState::new()));
    tokio::spawn(async move {
        rocket::ignite()
        .manage(karl_path)
        .manage(base_domain)
        .manage(hosts)
        .manage(sensors)
        .manage(sessions)
        .mount("/", routes![index, confirm_host, confirm_client])
        .mount("/", routes![client::storage, client::proxy_get])
        .attach(Template::custom(|engines| {
            engines.handlebars.register_helper("request", Box::new(request_helper));
            engines.handlebars.register_helper("heartbeat", Box::new(heartbeat_helper));
        }))
        .launch();
    });
}
