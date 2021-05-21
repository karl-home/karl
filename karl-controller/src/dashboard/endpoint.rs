// use std::collections::{HashMap};
use std::sync::{Arc, Mutex};
use rocket::State;
use rocket::http::Status;
use rocket_contrib::json::Json;
use crate::controller::{Controller, HostScheduler};
use super::graph::*;

#[get("/graph")]
pub fn get_graph(
    controller: State<Arc<Mutex<Controller>>>,
) -> Json<GraphJson> {
    Json(GraphJson::new(&controller.lock().unwrap()))
}

#[post("/graph", format = "json", data = "<graph>")]
pub fn save_graph(
    graph: Json<GraphJson>,
    controller: State<Arc<Mutex<Controller>>>,
) -> Status {
	let controller = controller.lock().unwrap();
	let old_graph = GraphJson::new(&controller);
	let deltas = old_graph.calculate_delta(&graph);
	debug!("{:?}", deltas);
	Status::Ok
}

#[post("/module/<id>")]
pub fn spawn_module(
    id: String,
    controller: State<Arc<Mutex<Controller>>>,
) -> Status {
    // controller.lock().unwrap().runner.spawn_module_id(id).await.unwrap();
    // Status:: Ok
    unimplemented!()
}

#[allow(non_snake_case)]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SensorResultJson {
    sensor: SensorJson,
    attestation: String,
}

#[post("/sensor/confirm/<id>")]
pub fn confirm_sensor(
    id: String,
    controller: State<Arc<Mutex<Controller>>>,
) -> Status {
    let controller = controller.lock().unwrap();
    let mut sensors = controller.sensors.lock().unwrap();
    if let Some(sensor) = sensors.get_sensor(&id) {
        if sensor.confirmed {
            warn!("attempted to confirm already confirmed sensor: {:?}", id);
            Status::Conflict
        } else {
            sensors.confirm_sensor(&id);
            Status::Ok
        }
    } else {
        warn!("attempted to confirm nonexistent sensor: {:?}", id);
        Status::NotFound
    }
}

#[post("/sensor/cancel/<id>")]
pub fn cancel_sensor(
    id: String,
    controller: State<Arc<Mutex<Controller>>>,
) -> Status {
    let controller = controller.lock().unwrap();
    let mut sensors = controller.sensors.lock().unwrap();
    if let Some(removed) = sensors.remove_sensor(&id) {
        info!("removed sensor {:?}", removed);
        Status::Ok
    } else {
        warn!("cannot remove sensor with id {}: does not exist", id);
        Status::NotFound
    }
}

#[get("/sensors")]
pub fn get_sensors(
    controller: State<Arc<Mutex<Controller>>>,
) -> Result<Json<Vec<SensorResultJson>>, Status> {
    Ok(Json(controller.lock().unwrap().sensors.lock().unwrap().list_sensors()
        .iter()
        .filter(|sensor| !sensor.confirmed)
        .map(|sensor| SensorResultJson {
            sensor: {
                // TODO: descriptions
                let state_keys = sensor.keys.iter()
                    .map(|key| (key.clone(), "-".to_string())).collect();
                let returns = sensor.returns.iter()
                    .map(|ret| (ret.clone(), "-".to_string())).collect();
                SensorJson {
                    id: sensor.id.clone(),
                    stateKeys: state_keys,
                    returns,
                }
            },
            attestation: "QWERTY1234".to_string(), // TODO
        })
        .collect()
    ))
}

#[allow(non_snake_case)]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct HostResultJson {
    confirmed: Vec<HostJson>,
    unconfirmed: Vec<String>,
}

#[allow(non_snake_case)]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct HostJson {
    id: String,
    activeModules: u32,
    online: bool,
}

#[post("/host/confirm/<id>")]
pub fn confirm_host(
    id: String,
    hosts: State<Arc<Mutex<HostScheduler>>>,
) {
    hosts.lock().unwrap().confirm_host(&id);
}

#[post("/host/cancel/<id>")]
pub fn cancel_host(
    id: String,
    hosts: State<Arc<Mutex<HostScheduler>>>,
) -> Status {
    if hosts.lock().unwrap().remove_host(&id) {
        Status::Ok
    } else {
        Status::NotFound
    }
}

#[get("/hosts")]
pub fn get_hosts(
    hosts: State<Arc<Mutex<HostScheduler>>>,
) -> Result<Json<HostResultJson>, Status> {
    let mut res = HostResultJson::default();
    for host in hosts.lock().unwrap().hosts() {
        if host.confirmed {
            res.confirmed.push(HostJson {
                id: host.id,
                activeModules: host.md.active_requests.len() as _,
                online: host.md.last_msg.elapsed().as_secs() <=
                    2 * karl_common::HEARTBEAT_INTERVAL,
            })
        } else {
            res.unconfirmed.push(host.id)
        }
    }
    Ok(Json(res))
}
