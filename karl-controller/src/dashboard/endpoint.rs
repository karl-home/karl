// use std::collections::{HashMap};
use std::sync::{Arc, Mutex};
use rocket::State;
use rocket::http::Status;
use rocket_contrib::json::Json;
use crate::controller::{Controller, HostScheduler};

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

// #[get("/graph")]
// pub fn get_graph(
//     controller: State<Arc<Mutex<Controller>>>,
// ) -> Result<Json<GraphJson>, Status> {
//     // TODO: unimplemented
//     info!("get_graph");
//     let mut graph = GraphJson::default();
//     let controller = controller.lock().unwrap();
//     let sensors = controller.sensors.lock().unwrap();
//     let modules = controller.runner.hooks.lock().unwrap();
//     let watched_tags = controller.runner.watched_tags.read().unwrap();
//     graph.sensors = sensors.list_sensors()
//         .iter()
//         .filter(|sensor| sensor.confirmed)
//         .map(|sensor| {
//             // TODO: descriptions
//             let state_keys = sensor.keys.iter()
//                 .map(|key| (key.clone(), "-".to_string())).collect();
//             let returns = sensor.returns.iter()
//                 .map(|ret| (ret.clone(), "-".to_string())).collect();
//             SensorJson {
//                 id: sensor.id.clone(),
//                 stateKeys: state_keys,
//                 returns,
//             }
//         })
//         .collect();
//     graph.moduleIds = modules.iter()
//         .map(|(hook_id, hook)| {
//             ModuleJson {
//                 localId: hook_id.to_string(),
//                 globalId: hook.global_hook_id.clone(),
//                 params: hook.params.keys().map(|x| x.to_string()).collect(),
//                 returns: hook.returns.keys().map(|x| x.to_string()).collect(),
//             }
//         })
//         .collect();

//     let entity_index = {
//         let mut map = HashMap::new();
//         for i in 0..graph.sensors.len() {
//             map.insert(graph.sensors.get(i).unwrap().id.clone(), i);
//         }
//         for i in 0..graph.moduleIds.len() {
//             let index = i + graph.sensors.len();
//             map.insert(graph.moduleIds.get(i).unwrap().localId.clone(), index);
//         }
//         map
//     };
//     let tag_map: HashMap<String, (u32, u32)> = graph.moduleIds.iter()
//         .flat_map(|module| {
//             let in_id = *entity_index.get(&module.localId).unwrap() as u32;
//             module.params.iter().enumerate().map(move |(in_index, in_tag)| {
//                 (in_tag.to_string(), (in_id, in_index as u32))
//             })
//         })
//         .collect();

//     // TODO: the most horrific implementation
//     for module in &graph.moduleIds {
//         let out_id = *entity_index.get(&module.localId).unwrap();
//         let out_returns = &modules.get(&module.localId).unwrap().returns;
//         for out_index in 0..module.returns.len() {
//             let out_tag = module.returns.get(out_index).unwrap();
//             let in_tags = out_returns.get(out_tag).unwrap();
//             for in_tag in in_tags {
//                 if in_tag.chars().next() == Some('#') {
//                     let mut split = in_tag.split(".");
//                     let sensor = split.next().unwrap();
//                     let key = split.next().unwrap();
//                     let sensor_id = *entity_index.get(&sensor[1..]).unwrap();
//                     let sensor_index = graph.sensors[sensor_id].stateKeys
//                         .iter()
//                         .enumerate()
//                         .filter(|(_, sensor)| sensor.0 == key)
//                         .map(|(index, _)| index)
//                         .next()
//                         .unwrap();
//                     graph.stateEdges.push((
//                         out_id as u32,
//                         out_index as u32,
//                         sensor_id as u32,
//                         sensor_index as u32,
//                     ));
//                 } else {
//                     let stateless = watched_tags
//                         .get(out_tag)
//                         .map(|hook_ids| hook_ids.contains(&module.localId))
//                         .unwrap_or(false);
//                     let (module_id, module_index) = tag_map.get(in_tag).unwrap();
//                     graph.dataEdges.push((
//                         stateless,
//                         out_id as u32,
//                         out_index as u32,
//                         *module_id as u32,
//                         *module_index as u32,
//                     ))
//                 }
//             }
//         }
//     }
//     for sensor in &graph.sensors {
//         let out_id = *entity_index.get(&sensor.id).unwrap();
//         let out_returns = sensors.tags(&sensor.id).unwrap();
//         for out_index in 0..sensor.returns.len() {
//             let out_tag = &sensor.returns.get(out_index).unwrap().0;
//             let in_tags = out_returns.get_output_tags(out_tag).unwrap();
//             for in_tag in in_tags {
//                 let stateless = watched_tags
//                     .get(out_tag)
//                     .map(|hook_ids| hook_ids.contains(&sensor.id))
//                     .unwrap_or(false);
//                 let (module_id, module_index) = tag_map.get(in_tag).unwrap();
//                 graph.dataEdges.push((
//                     stateless,
//                     out_id as u32,
//                     out_index as u32,
//                     *module_id as u32,
//                     *module_index as u32,
//                 ))
//             }
//         }
//     }
//     graph.networkEdges = modules.iter()
//         .flat_map(|(hook_id, hook)| {
//             let index = *entity_index.get(hook_id).unwrap() as u32;
//             hook.network_perm.clone().into_iter().map(move |domain| (index, domain))
//         })
//         .collect();
//     graph.intervals = modules.iter()
//         .filter_map(|(hook_id, hook)| hook.interval.map(|int| (hook_id, int)))
//         .map(|(hook_id, interval)| {
//             let index = *entity_index.get(hook_id).unwrap() as u32;
//             (index, interval)
//         })
//         .collect();
//     Ok(Json(graph))
// }

// #[derive(Default)]
// struct Edges {
//     data: HashMap<String, HashMap<String, Vec<(bool, String, String)>>>,
//     state: HashMap<String, HashMap<String, Vec<(String, String)>>>,
// }

// impl Edges {
//     pub fn add_data_edge(
//         &mut self,
//         stateless: bool,
//         out_id: &str,
//         out_name: &str,
//         in_id: &str,
//         in_name: &str,
//     ) {
//         if !self.data.contains_key(out_id) {
//             self.data.insert(out_id.to_string(), HashMap::new());
//         }
//         if !self.data.get(out_id).unwrap().contains_key(out_name) {
//             self.data.get_mut(out_id).unwrap().insert(out_name.to_string(), vec![]);
//         }
//         self.data
//             .get_mut(out_id).unwrap()
//             .get_mut(out_name).unwrap()
//             .push((stateless, in_id.to_string(), in_name.to_string()));
//     }

//     pub fn add_state_edge(
//         &mut self,
//         out_id: &str,
//         out_name: &str,
//         in_id: &str,
//         in_name: &str,
//     ) {
//         if !self.state.contains_key(out_id) {
//             self.state.insert(out_id.to_string(), HashMap::new());
//         }
//         if !self.state.get(out_id).unwrap().contains_key(out_name) {
//             self.state.get_mut(out_id).unwrap().insert(out_name.to_string(), vec![]);
//         }
//         self.state
//             .get_mut(out_id).unwrap()
//             .get_mut(out_name).unwrap()
//             .push((in_id.to_string(), in_name.to_string()));
//     }

//     pub fn remove_data_edge(
//         &mut self,
//         stateless: bool,
//         out_id: &str,
//         out_name: &str,
//         in_id: &str,
//         in_name: &str,
//     ) {
//         let index = self.data_edge_index(stateless, out_id, out_name, in_id, in_name).unwrap();
//         self.data.get_mut(out_id).unwrap().get_mut(out_name).unwrap().remove(index);
//         if self.data.get(out_id).unwrap().get(out_name).unwrap().is_empty() {
//             self.data.get_mut(out_id).unwrap().remove(out_name);
//         }
//         if self.data.get(out_id).unwrap().is_empty() {
//             self.data.remove(out_id);
//         }
//     }

//     pub fn remove_state_edge(
//         &mut self,
//         out_id: &str,
//         out_name: &str,
//         in_id: &str,
//         in_name: &str,
//     ) {
//         let index = self.state_edge_index(out_id, out_name, in_id, in_name).unwrap();
//         self.data.get_mut(out_id).unwrap().get_mut(out_name).unwrap().remove(index);
//         if self.data.get(out_id).unwrap().get(out_name).unwrap().is_empty() {
//             self.data.get_mut(out_id).unwrap().remove(out_name);
//         }
//         if self.data.get(out_id).unwrap().is_empty() {
//             self.data.remove(out_id);
//         }
//     }

//     pub fn data_edge_index(
//         &mut self,
//         stateless: bool,
//         out_id: &str,
//         out_name: &str,
//         in_id: &str,
//         in_name: &str,
//     ) -> Option<usize> {
//         let edges = self.data.get(out_id).unwrap().get(out_name).unwrap();
//         for i in 0..edges.len() {
//             let (a, b, c) = edges.get(i).unwrap();
//             if *a == stateless && b == in_id && c == in_name {
//                 return Some(i);
//             }
//         }
//         None
//     }

//     pub fn state_edge_index(
//         &mut self,
//         out_id: &str,
//         out_name: &str,
//         in_id: &str,
//         in_name: &str,
//     ) -> Option<usize> {
//         let edges = self.state.get(out_id).unwrap().get(out_name).unwrap();
//         for i in 0..edges.len() {
//             let (b, c) = edges.get(i).unwrap();
//             if b == in_id && c == in_name {
//                 return Some(i);
//             }
//         }
//         None
//     }
// }

// #[post("/graph", format = "json", data = "<graph>")]
// pub fn save_graph(
//     mut graph: Json<GraphJson>,
//     controller: State<Arc<Mutex<Controller>>>,
// ) -> Status {
//     info!("save_graph");
//     info!("{:?}", graph);
//     // TODO: sensors
//     let controller = controller.lock().unwrap();

//     // Register the same modules
//     let new_modules: HashMap<_, _> =
//         graph.moduleIds.drain(..).map(|m| (m.localId, m.globalId)).collect();
//     {
//         let mut modules = controller.modules.lock().unwrap();
//         let (modules_to_remove, modules_to_add) = modules.delta(new_modules);
//         for module in modules_to_remove {
//             if modules.remove(module).is_err() {
//                 return Status::BadRequest;
//             }
//         }
//         for module in modules_to_add {
//             if modules.add(module).is_err() {
//                 return Status::BadRequest
//             }
//         }
//     }

//     // entity ID, input names, output names
//     let mut entity_map: HashMap<u32, (String, Vec<String>, Vec<String>)> = HashMap::new();
//     for i in 0..graph.sensors.len() {
//         let index = i as u32;
//         let sensor = graph.sensors.get(i).unwrap();
//         let inputs: Vec<_> = sensor.stateKeys.iter()
//             .map(|(key, _)| format!("#{}.{}", sensor.id, key)).collect();
//         let outputs: Vec<_> = sensor.returns.iter()
//             .map(|(ret, _)| ret.to_string()).collect();
//         entity_map.insert(index, (sensor.id.to_string(), inputs, outputs));
//     }
//     for i in 0..graph.moduleIds.len() {
//         let index = (i + graph.sensors.len()) as u32;
//         let module = graph.moduleIds.get(i).unwrap();
//         let inputs: Vec<_> = module.params.iter()
//             .map(|param| format!("#{}.{}", module.localId, param)).collect();
//         let outputs: Vec<_> = module.returns.iter()
//             .map(|ret| ret.to_string()).collect();
//         entity_map.insert(index, (module.localId.to_string(), inputs, outputs));
//     }

//     let (edges_to_add, edges_to_remove) = {
//         let modules = controller.runner.hooks.lock().unwrap();
//         let sensors = controller.sensors.lock().unwrap();
//         let watched_tags = controller.runner.watched_tags.read().unwrap();
//         let mut edges_to_add: Edges = Default::default();
//         let mut edges_to_remove: Edges = Default::default();
//         for (stateless, out_id, out_index, in_id, in_index) in graph.dataEdges.drain(..) {
//             let (out_id, _, outputs) = entity_map.get(&out_id).unwrap();
//             let (in_id, inputs, _) = entity_map.get(&in_id).unwrap();
//             let output = &outputs[out_index as usize];
//             let input = &inputs[in_index as usize];
//             edges_to_add.add_data_edge(stateless, out_id, output, in_id, input);
//         }
//         for (out_id, out_index, in_id, in_index) in graph.stateEdges.drain(..) {
//             let (out_id, _, outputs) = entity_map.get(&out_id).unwrap();
//             let (in_id, inputs, _) = entity_map.get(&in_id).unwrap();
//             let output = &outputs[out_index as usize];
//             let input = &inputs[in_index as usize];
//             edges_to_add.add_state_edge(out_id, output, in_id, input);
//         }

//         let mut tag_map: HashMap<String, (String, String)> = HashMap::new();
//         for (module_id, module) in modules.iter() {
//             for (param, tag) in module.params.iter() {
//                 if let Some(tag) = tag {
//                     tag_map.insert(tag.to_string(), (module_id.to_string(), param.to_string()));
//                 }
//             }
//         }
//         for (o1, module) in modules.iter() {
//             for (o2, tags) in module.returns.iter() {
//                 for tag in tags {
//                     if let Some((i1, i2)) = tag_map.get(tag) {
//                         let stateless = watched_tags
//                             .get(tag)
//                             .map(|hook_ids| hook_ids.contains(&i1))
//                             .unwrap_or(false);
//                         if edges_to_add.data_edge_index(stateless, o1, o2, i1, i2).is_some() {
//                             edges_to_add.remove_data_edge(stateless, o1, o2, i1, i2);
//                         } else {
//                             edges_to_remove.add_data_edge(stateless, o1, o2, i1, i2);
//                         }
//                     } else {
//                         let mut split = tag.split(".");
//                         let i1 = split.next().unwrap();
//                         let i2 = split.next().unwrap();
//                         if edges_to_add.state_edge_index(o1, o2, &i1[1..], i2).is_some() {
//                             edges_to_add.remove_state_edge(o1, o2, &i1[1..], i2);
//                         } else {
//                             edges_to_remove.add_state_edge(o1, o2, &i1[1..], i2);
//                         }
//                     }
//                 }
//             }
//         }
//         for sensor in sensors.list_sensors() {
//             let o1 = &sensor.id;
//             for o2 in sensor.returns.iter() {
//                 let tags = sensors.tags(o1).unwrap().get_output_tags(o2).unwrap();
//                 for tag in tags {
//                     let (i1, i2) = tag_map.get(tag).unwrap();
//                     let stateless = watched_tags
//                         .get(tag)
//                         .map(|hook_ids| hook_ids.contains(&i1))
//                         .unwrap_or(false);
//                     if edges_to_add.data_edge_index(stateless, o1, o2, i1, i2).is_some() {
//                         edges_to_add.remove_data_edge(stateless, o1, o2, i1, i2);
//                     } else {
//                         edges_to_remove.add_data_edge(stateless, o1, o2, i1, i2);
//                     }
//                 }
//             }
//         }
//         (edges_to_add, edges_to_remove)
//     };

//     {
//         let mut modules = controller.runner.hooks.lock().unwrap();
//         let mut sensors = controller.sensors.lock().unwrap();
//         for (o1, map) in edges_to_remove.data.into_iter() {
//             for (o2, array) in map.into_iter() {
//                 for (stateless, i1, i2) in array.into_iter() {
//                     warn!("unimplemented: remove data edge {} {}.{} -> {}.{}",
//                         stateless, o1, o2, i1, i2);
//                 }
//             }
//         }
//         for (o1, map) in edges_to_remove.state.into_iter() {
//             for (o2, array) in map.into_iter() {
//                 for (i1, i2) in array.into_iter() {
//                     warn!("unimplemented: remove state edge {}.{} -> {}.{}",
//                         o1, o2, i1, i2);
//                 }
//             }
//         }
//         for (o1, map) in edges_to_add.data.into_iter() {
//             for (o2, array) in map.into_iter() {
//                 for (stateless, i1, i2) in array.into_iter() {
//                     if let Err(e) = controller.add_data_edge(
//                         stateless, o1.clone(), o2.clone(), i1, i2,
//                         &mut modules, &mut sensors,
//                     ) {
//                         error!("error adding data edge: {:?}", e);
//                         return Status::BadRequest;
//                     }
//                 }
//             }
//         }
//         for (o1, map) in edges_to_add.state.into_iter() {
//             for (o2, array) in map.into_iter() {
//                 for (i1, i2) in array.into_iter() {
//                     if let Err(e) = controller.add_state_edge(
//                         o1.clone(), o2.clone(), i1, i2,
//                         &mut modules, &mut sensors,
//                     ) {
//                         error!("error adding state edge: {:?}", e);
//                         return Status::BadRequest;
//                     }
//                 }
//             }
//         }
//     }

//     let (intervals_to_add, intervals_to_remove) = {
//         let mut modules = controller.runner.hooks.lock().unwrap();
//         // Set module network edges
//         for module in modules.values_mut() {
//             module.network_perm = vec![];
//         }
//         for (module_index, domain) in graph.networkEdges.drain(..) {
//             let (module_id, _, _) = entity_map.get(&module_index).unwrap();
//             modules.get_mut(module_id).unwrap().network_perm.push(domain);
//         }

//         // Set module intervals
//         let mut intervals_to_add = graph.intervals.drain(..)
//             .map(|(module_index, seconds)| {
//                 let (module_id, _, _) = entity_map.get(&module_index).unwrap();
//                 (module_id.to_string(), seconds)
//             })
//             .collect::<HashMap<_, _>>();
//         let mut intervals_to_remove: Vec<String> = vec![];
//         for (module_id, module) in modules.iter() {
//             if let Some(old_duration) = module.interval {
//                 if let Some(new_duration) = intervals_to_add.get(module_id) {
//                     if old_duration == *new_duration {
//                         intervals_to_add.remove(module_id);
//                     } else {
//                         intervals_to_remove.push(module_id.to_string());
//                     }
//                 } else {
//                     intervals_to_remove.push(module_id.to_string());
//                 }
//             }
//         }
//         (intervals_to_add, intervals_to_remove)
//     };

//     for module_id in intervals_to_remove {
//         // TODO: remove interval
//         warn!("unimplemented: remove interval {}", module_id);
//     }
//     for (module_id, duration) in intervals_to_add {
//         if let Err(e) = controller.runner.set_interval(
//             module_id.to_string(),
//             duration,
//         ) {
//             error!("error setting interval {}: {:?}", module_id, e);
//             return Status::BadRequest;
//         }
//     }

//     Status::Ok
// }

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
