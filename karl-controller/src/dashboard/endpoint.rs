// use std::collections::{HashMap};
use std::sync::{Arc, Mutex};
use rocket::State;
use rocket::http::Status;
use rocket_contrib::json::Json;
use crate::controller::{Controller, HostScheduler};
use karl_common::Error;
use super::graph::*;

#[get("/graph")]
pub fn get_graph(
    controller: State<Arc<Mutex<Controller>>>,
) -> Json<GraphJson> {
    Json(controller.lock().unwrap().policies.read().unwrap().json.clone())
}

#[get("/policy")]
pub fn get_policy(
    controller: State<Arc<Mutex<Controller>>>,
) -> Json<PolicyJson> {
    Json(PolicyJson::new(&controller.lock().unwrap()))
}

fn apply_deltas(
    c: &mut Controller,
    deltas: Vec<Delta>,
) -> Result<(), Error> {
    let mut modules = c.modules.write().unwrap();
    let mut sensors = c.sensors.lock().unwrap();
    for delta in deltas {
        match delta {
        Delta::AddModule { global_id, id } => {
            c.add_module(&global_id, &id, &mut modules)?;
        }
        Delta::RemoveModule { id } => {
            c.remove_module(id, &mut modules)?;
        }
        Delta::AddDataEdge { stateless, src_id, src_name, dst_id, dst_name } => {
            c.add_data_edge(
                stateless, src_id, src_name, dst_id, dst_name,
                &mut modules, &mut sensors,
            )?;
        }
        Delta::RemoveDataEdge { stateless, src_id, src_name, dst_id, dst_name } => {
            c.remove_data_edge(
                stateless, src_id, src_name, dst_id, dst_name,
                &mut modules, &mut sensors,
            )?;
        }
        Delta::AddStateEdge { src_id, src_name, dst_id, dst_name } => {
            c.add_state_edge(
                src_id, src_name, dst_id, dst_name,
                &mut modules, &sensors,
            )?;
        }
        Delta::RemoveStateEdge { src_id, src_name, dst_id, dst_name } => {
            c.remove_state_edge(
                src_id, src_name, dst_id, dst_name,
                &mut modules,
            )?;
        }
        Delta::SetNetworkEdges { id, domains } => {
            c.set_network_edges(id, domains, &mut modules)?;
        }
        Delta::SetInterval { id, duration } => {
            c.set_interval(id, duration, &mut modules)?;
        }
        }
    }
    Ok(())
}

#[post("/graph", format = "json", data = "<graph>")]
pub fn save_graph(
    graph: Json<GraphJson>,
    controller: State<Arc<Mutex<Controller>>>,
) -> Status {
	let mut c = controller.lock().unwrap();
	let old_graph = GraphJson::new(&c);
	let deltas = match old_graph.calculate_delta(&graph) {
        Ok(deltas) => deltas,
        Err(e) => {
            error!("invalid graph: {:?}", e);
            return Status::BadRequest;
        }
    };
    match apply_deltas(&mut c, deltas) {
        Ok(()) => {
            c.policies.write().unwrap().save_graph(graph.into_inner());
            Status::Ok
        },
        Err(e) => {
            error!("error saving graph: {:?}", e);
            Status::InternalServerError
        }
    }
}

#[post("/policy", format = "json", data = "<policy>")]
pub fn save_policy(
    policy: Json<PolicyJson>,
    controller: State<Arc<Mutex<Controller>>>,
) -> Status {
    let c = controller.lock().unwrap();
    let mut policies = c.policies.write().unwrap();
    if let Err(e) = policies.save_policies(policy.into_inner()) {
        error!("error saving policies: {:?}", e);
        Status::BadRequest
    } else {
        Status::Ok
    }
}

#[post("/module/<id>")]
pub fn spawn_module(
    id: String,
    controller: State<Arc<Mutex<Controller>>>,
) -> Status {
    warn!("START LivestreamOn: {:?}", std::time::Instant::now());
    warn!("START (chrono) LivestreamOn: {:?}", chrono::Utc::now().time());
    let c = controller.lock().unwrap();
    c.handle.block_on(c.runner.spawn_module(id));
    Status::Ok
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

#[get("/tags")]
pub fn list_tags(
    controller: State<Arc<Mutex<Controller>>>,
) -> Result<Json<Vec<String>>, Status> {
    let res = controller.lock().unwrap()
        .data_sink.read().unwrap()
        .list_tags();
    Ok(Json(res))
}

#[get("/tags/<tag>")]
pub fn read_tag(
    tag: String,
    controller: State<Arc<Mutex<Controller>>>,
) -> Result<Json<crate::controller::data::GetDataResult>, Status> {
    let res = controller.lock().unwrap()
        .data_sink.read().unwrap()
        .get_data_inner(&tag, None, None);
    match res {
        Ok(res) => Ok(Json(res)),
        Err(e) => {
            error!("{:?}", e);
            Err(Status::NotFound)
        }
    }
}
