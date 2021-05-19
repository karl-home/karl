use std::collections::{HashMap};
use std::sync::{Arc, Mutex, RwLock};
use rocket::State;
use rocket::http::Status;
use rocket_contrib::json::Json;
use tokio::sync::mpsc;
use karl_common::*;
use crate::controller::{QueuedHook, HookRunner, HostScheduler};

#[allow(non_snake_case)]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GraphJson {
    // sensors indexed 0 to n-1, where n is the number of sensors
    sensors: Vec<SensorJson>,
    // modules indexed n to n+m-1, where m is the number of modules
    moduleIds: Vec<ModuleJson>,
    // stateless, out_id, out_red, module_id, module_param
    dataEdges: Vec<(bool, u32, u32, u32, u32)>,
    // module_id, module_ret, sensor_id, sensor_key
    stateEdges: Vec<(u32, u32, u32, u32)>,
    // module_id, domain
    networkEdges: Vec<(u32, String)>,
    // module_id, duration_s
    intervals: Vec<(u32, u32)>,
}

#[allow(non_snake_case)]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SensorJson {
    id: String,
    stateKeys: Vec<(String, String)>,
    returns: Vec<(String, String)>,
}

#[allow(non_snake_case)]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ModuleJson {
    localId: String,
    globalId: String,
    params: Vec<String>,
    returns: Vec<String>,
}

#[get("/graph")]
pub fn get_graph(
    sensors: State<Arc<Mutex<HashMap<SensorToken, Client>>>>,
    modules: State<Arc<Mutex<HashMap<HookID, Hook>>>>,
    watched_tags: State<Arc<RwLock<HashMap<String, Vec<HookID>>>>>,
) -> Result<Json<GraphJson>, Status> {
    // TODO: unimplemented
    info!("get_graph");
    let mut graph = GraphJson::default();
    let sensors = sensors.lock().unwrap();
    let modules = modules.lock().unwrap();
    let watched_tags = watched_tags.read().unwrap();
    graph.sensors = sensors.values()
        .filter(|sensor| sensor.confirmed)
        .map(|sensor| {
            // TODO: descriptions
            let state_keys = sensor.keys.iter()
                .map(|key| (key.clone(), "-".to_string())).collect();
            let returns = sensor.returns.keys()
                .map(|ret| (ret.clone(), "-".to_string())).collect();
            SensorJson {
                id: sensor.id.clone(),
                stateKeys: state_keys,
                returns,
            }
        })
        .collect();
    graph.moduleIds = modules.iter()
        .map(|(hook_id, hook)| {
            ModuleJson {
                localId: hook_id.to_string(),
                globalId: hook.global_hook_id.clone(),
                params: hook.params.keys().map(|x| x.to_string()).collect(),
                returns: hook.returns.keys().map(|x| x.to_string()).collect(),
            }
        })
        .collect();

    let entity_index = {
        let mut map = HashMap::new();
        for i in 0..graph.sensors.len() {
            map.insert(graph.sensors.get(i).unwrap().id.clone(), i);
        }
        for i in 0..graph.moduleIds.len() {
            let index = i + graph.sensors.len();
            map.insert(graph.moduleIds.get(i).unwrap().localId.clone(), index);
        }
        map
    };
    let tag_map: HashMap<String, (u32, u32)> = graph.moduleIds.iter()
        .flat_map(|module| {
            let in_id = *entity_index.get(&module.localId).unwrap() as u32;
            module.params.iter().enumerate().map(move |(in_index, in_tag)| {
                (in_tag.to_string(), (in_id, in_index as u32))
            })
        })
        .collect();

    // TODO: the most horrific implementation
    for module in &graph.moduleIds {
        let out_id = *entity_index.get(&module.localId).unwrap();
        let out_returns = &modules.get(&module.localId).unwrap().returns;
        for out_index in 0..module.returns.len() {
            let out_tag = module.returns.get(out_index).unwrap();
            let in_tags = out_returns.get(out_tag).unwrap();
            for in_tag in in_tags {
                if in_tag.chars().next() == Some('#') {
                    let mut split = in_tag.split(".");
                    let sensor = split.next().unwrap();
                    let key = split.next().unwrap();
                    let sensor_id = *entity_index.get(sensor).unwrap();
                    let sensor_index = graph.sensors[sensor_id].stateKeys
                        .iter()
                        .enumerate()
                        .filter(|(_, sensor)| sensor.0 == key)
                        .map(|(index, _)| index)
                        .next()
                        .unwrap();
                    graph.stateEdges.push((
                        out_id as u32,
                        out_index as u32,
                        sensor_id as u32,
                        sensor_index as u32,
                    ));
                } else {
                    let stateless = watched_tags
                        .get(out_tag)
                        .map(|hook_ids| hook_ids.contains(&module.localId))
                        .unwrap_or(false);
                    let (module_id, module_index) = tag_map.get(in_tag).unwrap();
                    graph.dataEdges.push((
                        stateless,
                        out_id as u32,
                        out_index as u32,
                        *module_id as u32,
                        *module_index as u32,
                    ))
                }
            }
        }
    }
    for sensor in &graph.sensors {
        let out_id = *entity_index.get(&sensor.id).unwrap();
        let out_returns = &sensors.get(&sensor.id).unwrap().returns;
        for out_index in 0..sensor.returns.len() {
            let out_tag = &sensor.returns.get(out_index).unwrap().0;
            let in_tags = out_returns.get(out_tag).unwrap();
            for in_tag in in_tags {
                let stateless = watched_tags
                    .get(out_tag)
                    .map(|hook_ids| hook_ids.contains(&sensor.id))
                    .unwrap_or(false);
                let (module_id, module_index) = tag_map.get(in_tag).unwrap();
                graph.dataEdges.push((
                    stateless,
                    out_id as u32,
                    out_index as u32,
                    *module_id as u32,
                    *module_index as u32,
                ))
            }
        }
    }
    graph.networkEdges = modules.iter()
        .flat_map(|(hook_id, hook)| {
            let index = *entity_index.get(hook_id).unwrap() as u32;
            hook.network_perm.clone().into_iter().map(move |domain| (index, domain))
        })
        .collect();
    graph.intervals = modules.iter()
        .filter_map(|(hook_id, hook)| hook.interval.map(|int| (hook_id, int)))
        .map(|(hook_id, interval)| {
            let index = *entity_index.get(hook_id).unwrap() as u32;
            (index, interval)
        })
        .collect();
    Ok(Json(graph))
}

#[post("/graph", format = "json", data = "<graph>")]
pub fn save_graph(
    mut graph: Json<GraphJson>,
    sensors: State<Arc<Mutex<HashMap<SensorToken, Client>>>>,
    modules: State<Arc<Mutex<HashMap<HookID, Hook>>>>,
    watched_tags: State<Arc<RwLock<HashMap<String, Vec<HookID>>>>>,
    hook_tx: State<mpsc::Sender<QueuedHook>>,
) -> Status {
    info!("save_graph");
    info!("{:?}", graph);
    // TODO: sensors
    let _sensors = sensors.lock().unwrap();
    let mut modules = modules.lock().unwrap();
    let mut watched_tags = watched_tags.write().unwrap();

    // entity ID, input names, output names
    let mut entity_map: HashMap<u32, (String, Vec<String>, Vec<String>)> = HashMap::new();
    for i in 0..graph.sensors.len() {
        let index = i as u32;
        let sensor = graph.sensors.get(i).unwrap();
        let inputs: Vec<_> = sensor.stateKeys.iter()
            .map(|(key, _)| format!("#{}.{}", sensor.id, key)).collect();
        let outputs: Vec<_> = sensor.returns.iter()
            .map(|(ret, _)| ret.to_string()).collect();
        entity_map.insert(index, (sensor.id.to_string(), inputs, outputs));
    }
    for i in 0..graph.moduleIds.len() {
        let index = (i + graph.sensors.len()) as u32;
        let module = graph.moduleIds.get(i).unwrap();
        let inputs: Vec<_> = module.params.iter()
            .map(|param| format!("#{}.{}", module.localId, param)).collect();
        let outputs: Vec<_> = module.returns.iter()
            .map(|ret| ret.to_string()).collect();
        entity_map.insert(index, (module.localId.to_string(), inputs, outputs));
    }

    {
        // Register the same modules
        let new_modules: HashMap<_, _> =
            graph.moduleIds.drain(..).map(|m| (m.localId, m.globalId)).collect();
        let modules_to_remove: Vec<_> = modules.keys()
            .filter(|&m| !new_modules.contains_key(m))
            .map(|m| m.to_string())
            .collect();
        let modules_to_add: Vec<_> = new_modules.keys()
            .filter(|&m| !modules.contains_key(m))
            .map(|m| m.to_string())
            .collect();
        for hook_id in &modules_to_remove {
            if let Err(e) = HookRunner::remove_hook(
                &mut modules,
                &mut watched_tags,
                hook_id.to_string(),
            ) {
                error!("error removing {}: {:?}", hook_id, e);
                return Status::BadRequest;
            }

        }
        for hook_id in &modules_to_add {
            if let Err(e) = HookRunner::register_hook(
                &mut modules,
                new_modules.get(hook_id).unwrap().clone(),
                hook_id.to_string(),
            ) {
                error!("error registering {}: {:?}", hook_id, e);
                return Status::BadRequest;
            }
        }
    }

    {
        // Set module network edges
        for module in modules.values_mut() {
            module.network_perm = vec![];
        }
        for (module_index, domain) in graph.networkEdges.drain(..) {
            let (module_id, _, _) = entity_map.get(&module_index).unwrap();
            modules.get_mut(module_id).unwrap().network_perm.push(domain);
        }

        // Set module intervals
        let mut intervals_to_add = graph.intervals.drain(..)
            .map(|(module_index, seconds)| {
                let (module_id, _, _) = entity_map.get(&module_index).unwrap();
                (module_id.to_string(), seconds)
            })
            .collect::<HashMap<_, _>>();
        let mut intervals_to_remove: Vec<String> = vec![];
        for (module_id, module) in modules.iter() {
            if let Some(old_duration) = module.interval {
                if let Some(new_duration) = intervals_to_add.get(module_id) {
                    if old_duration == *new_duration {
                        intervals_to_add.remove(module_id);
                    } else {
                        intervals_to_remove.push(module_id.to_string());
                    }
                } else {
                    intervals_to_remove.push(module_id.to_string());
                }
            }
        }

        for module_id in intervals_to_remove {
            // TODO: remove interval
            warn!("unimplemented: remove interval {}", module_id);
        }
        for (module_id, duration) in intervals_to_add {
            if let Err(e) = HookRunner::set_interval(
                hook_tx.inner().clone(),
                &mut modules,
                module_id.to_string(),
                duration,
            ) {
                error!("error setting interval {}: {:?}", module_id, e);
                return Status::BadRequest;
            }
        }
    }

    Status::Ok
}

#[post("/module/<id>")]
pub fn spawn_module(id: String) -> Status {
    Status::NotImplemented
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
    sensors: State<Arc<Mutex<HashMap<SensorToken, Client>>>>,
) -> Status {
    let mut handle = sensors.lock().unwrap();
    let mut sensors = handle.iter_mut()
        .map(|(_, sensor)| sensor)
        .filter(|sensor| sensor.id == id);
    if let Some(sensor) = sensors.next() {
        if sensor.confirmed {
            warn!("attempted to confirm already confirmed sensor: {:?}", id);
            Status::Conflict
        } else {
            info!("confirmed sensor {:?}", id);
            sensor.confirmed = true;
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
    sensors: State<Arc<Mutex<HashMap<SensorToken, Client>>>>,
) -> Status {
    let mut handle = sensors.lock().unwrap();
    let sensors = handle.iter()
        .filter(|(_, sensor)| sensor.id == id)
        .map(|(token, _)| token.clone())
        .collect::<Vec<_>>();
    if !sensors.is_empty() {
        for token in sensors {
            info!("removed sensor {:?}", handle.remove(&token));
        }
        Status::Ok
    } else {
        warn!("cannot remove sensor with id {}: does not exist", id);
        Status::NotFound
    }
}

#[get("/sensors")]
pub fn get_sensors(
    sensors: State<Arc<Mutex<HashMap<SensorToken, Client>>>>,
) -> Result<Json<Vec<SensorResultJson>>, Status> {
    Ok(Json(sensors.lock().unwrap().values()
        .filter(|sensor| !sensor.confirmed)
        .map(|sensor| SensorResultJson {
            sensor: {
                // TODO: descriptions
                let state_keys = sensor.keys.iter()
                    .map(|key| (key.clone(), "-".to_string())).collect();
                let returns = sensor.returns.keys()
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
