//! Controller dashboard.
mod client;
mod helper;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;

use rocket::{self, State};
use rocket_contrib::templates::Template;
use tokio::runtime::Runtime;
use serde::Serialize;

use crate::common::ClientToken;
use crate::controller::{Host, Client};
use helper::*;

#[derive(Serialize)]
struct MainContext {
    title: &'static str,
    base_domain: String,
    hosts: Vec<Host>,
    clients: Vec<Client>,
}

#[derive(Serialize)]
struct AppContext {
    client_id: String,
    client_ip: String,
    files: Vec<PathBuf>,
}

// TODO: authenticate this request. Only the homeowner can access the page.
#[get("/")]
fn index(
    host_header: HostHeader,
    base_domain: State<String>,
    karl_path: State<PathBuf>,
    hosts: State<Arc<Mutex<HashMap<String, Host>>>>,
    clients: State<Arc<Mutex<HashMap<ClientToken, Client>>>>,
) -> Option<Template> {
    let client_id = match to_client_id(host_header, base_domain.to_string()) {
        Some(client_id) => client_id,
        None => {
            return Some(Template::render("index", &MainContext {
                title: "Hello",
                base_domain: base_domain.to_string(),
                hosts: hosts.lock().unwrap().values().map(
                    |host| host.clone()).collect(),
                clients: clients.lock().unwrap().values().map(
                    |client| client.clone()).collect(),
            }));
        },
    };

    let client_ip = {
        let mut client_ip = None;
        let clients = clients.lock().unwrap();
        for client in clients.values() {
            if client.name == client_id {
                client_ip = Some(format!("{}", client.addr));
                break;
            }
        }
        if let Some(client_ip) = client_ip {
            client_ip
        } else {
            return None;
        }
    };

    let files = {
        let storage_path = karl_path.join("storage").join(&client_id);
        let mut files = std::fs::read_dir(&storage_path).unwrap()
            .map(|res| res.unwrap().path())
            .map(|path| path.strip_prefix(&storage_path).unwrap().to_path_buf())
            .collect::<Vec<_>>();
        files.sort();
        files
    };
    debug!("files for client_id={}: {:?}", &client_id, files);
    Some(Template::render(
        client_id.clone(),
        &AppContext { client_id, client_ip, files },
    ))
}

// TODO: authenticate this request. Only the homeowner can call this.
#[post("/confirm/host/<service_name>")]
fn confirm_host(
    host_header: HostHeader,
    base_domain: State<String>,
    service_name: String,
    hosts: State<Arc<Mutex<HashMap<String, Host>>>>,
) {
    if to_client_id(host_header, base_domain.to_string()).is_some() {
        // Confirm must be to the base domain only.
        return;
    }
    let mut hosts = hosts.lock().unwrap();
    if let Some(host) = hosts.get_mut(&service_name) {
        if host.confirmed {
            warn!("attempted to confirm already confirmed host: {:?}", service_name);
        } else {
            info!("confirmed host {:?}", service_name);
            host.confirmed = true;
        }
    } else {
        warn!("attempted to confirm nonexistent host: {:?}", service_name);
    }
}

// TODO: authenticate this request. Only the homeowner can call this.
#[post("/confirm/client/<client_name>")]
fn confirm_client(
    host_header: HostHeader,
    base_domain: State<String>,
    client_name: String,
    clients: State<Arc<Mutex<HashMap<ClientToken, Client>>>>,
) {
    if to_client_id(host_header, base_domain.to_string()).is_some() {
        // Confirm must be to the base domain only.
        return;
    }
    let mut clients = clients.lock().unwrap();
    for (_, client) in clients.iter_mut() {
        if client.name != client_name {
            continue;
        }
        if client.confirmed {
            warn!("attempted to confirm already confirmed client: {:?}", client_name);
            return;
        } else {
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
    hosts: Arc<Mutex<HashMap<String, Host>>>,
    clients: Arc<Mutex<HashMap<ClientToken, Client>>>,
) {
    let base_domain = "karl.zapto.org".to_string();
    rt.spawn(async move {
        rocket::ignite()
        .manage(karl_path)
        .manage(base_domain)
        .manage(hosts)
        .manage(clients)
        .mount("/", routes![index, confirm_host, confirm_client])
        .mount("/api/", routes![client::storage, client::proxy_get])
        .attach(Template::custom(|engines| {
            engines.handlebars.register_helper("request", Box::new(request_helper));
        }))
        .launch();
    });
}
