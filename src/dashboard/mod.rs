//! Controller dashboard.
mod client;
mod helper;
mod cookie;

use std::fs;
use std::io::Write;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;

use rocket::{self, http::Cookies, State};
use rocket_contrib::templates::Template;
use tokio::runtime::Runtime;
use serde::Serialize;

use crate::common::ClientToken;
use crate::controller::{HostScheduler, types::{Host, Client}};
use helper::*;
use cookie::SessionState;

#[derive(Serialize)]
struct MainContext {
    title: &'static str,
    base_domain: String,
    hosts: Vec<Host>,
    clients: Vec<Client>,
}

/// Returns the client app if the page is a subdomain of the base domain,
/// otherwise returns the main controller dashboard.
///
/// The main dashboard visualizes the hosts and clients, and provides an
/// interface for the user to manually verify hosts and clients. Provides
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
    clients: State<Arc<Mutex<HashMap<ClientToken, Client>>>>,
    sessions: State<Arc<Mutex<SessionState>>>,
) -> Option<Template> {
    match to_client_id(&host_header, base_domain.to_string()) {
        Some(client_id) => client::index(client_id, karl_path, cookies, sessions),
        None => Some(Template::render("index", &MainContext {
            title: "Hello",
            base_domain: base_domain.to_string(),
            hosts: hosts.lock().unwrap().hosts(),
            clients: clients.lock().unwrap().values().map(
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
#[post("/confirm/client/<client_name>")]
fn confirm_client(
    host_header: HostHeader,
    karl_path: State<PathBuf>,
    base_domain: State<String>,
    client_name: String,
    clients: State<Arc<Mutex<HashMap<ClientToken, Client>>>>,
) {
    if to_client_id(&host_header, base_domain.to_string()).is_some() {
        // Confirm must be to the base domain only.
        return;
    }
    let mut clients = clients.lock().unwrap();
    for (token, client) in clients.iter_mut() {
        if client.name != client_name {
            continue;
        }
        if client.confirmed {
            warn!("attempted to confirm already confirmed client: {:?}", client_name);
            return;
        } else {
            // Create the client file if it does not already exist.
            let path = karl_path.join("clients.txt");
            let mut file = fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&path)
                .expect(&format!("unable to open clients file: {:?}", &path));
            let res = writeln!(file, "{}:{}={}", client.name, client.addr, token.0);
            if let Err(e) = res {
                error!("error serializing client {:?}: {:?}", client, e);
            }
            info!("confirmed client {:?}", client_name);
            client.confirmed = true;
            return;
        }
    }
    warn!("attempted to confirm nonexistent client: {:?}", client_name);
}

pub fn start(
    rt: &mut Runtime,
    karl_path: PathBuf,
    hosts: Arc<Mutex<HostScheduler>>,
    clients: Arc<Mutex<HashMap<ClientToken, Client>>>,
) {
    let base_domain = "karl.zapto.org".to_string();
    let sessions = Arc::new(Mutex::new(SessionState::new()));
    rt.spawn(async move {
        rocket::ignite()
        .manage(karl_path)
        .manage(base_domain)
        .manage(hosts)
        .manage(clients)
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
