use std::collections::{HashSet, HashMap};
use karl_common::*;
use crate::controller::{sensors, runner, Controller};

#[allow(non_snake_case)]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GraphJson {
    // sensors indexed 0 to n-1, where n is the number of sensors
    pub sensors: Vec<SensorJson>,
    // modules indexed n to n+m-1, where m is the number of modules
    pub moduleIds: Vec<ModuleJson>,
    // stateless, out_id, out_red, module_id, module_param
    pub dataEdges: DataEdges,
    // module_id, module_ret, sensor_id, sensor_key
    pub stateEdges: StateEdges,
    // module_id, domain
    pub networkEdges: Vec<(u32, String)>,
    // module_id, duration_s
    pub intervals: Vec<(u32, u32)>,
}

#[allow(non_snake_case)]
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SensorJson {
    pub id: String,
    pub stateKeys: Vec<(String, String)>,
    pub returns: Vec<(String, String)>,
}

#[allow(non_snake_case)]
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ModuleJson {
    pub localId: String,
    pub globalId: String,
    pub params: Vec<String>,
    pub returns: Vec<String>,
}

type DataEdges = Vec<(bool, u32, u32, u32, u32)>;
type StateEdges = Vec<(u32, u32, u32, u32)>;

impl GraphJson {
    fn parse_sensors(sensors: Vec<&sensors::Sensor>) -> Vec<SensorJson> {
        // TODO: sort sensors?
        sensors.iter()
            .filter(|s| s.confirmed)
            .map(|s| {
                // TODO: descriptions
                let state_keys = s.keys.iter()
                    .map(|key| (key.clone(), "-".to_string())).collect();
                let returns = s.returns.iter()
                    .map(|ret| (ret.clone(), "-".to_string())).collect();
                SensorJson {
                    id: s.id.clone(),
                    stateKeys: state_keys,
                    returns,
                }
            })
            .collect()
    }

    fn parse_modules(modules: Vec<(&ModuleID, &Module)>) -> Vec<ModuleJson> {
        // TODO: sort modules?
        modules.iter()
            .map(|(module_id, m)| {
                ModuleJson {
                    localId: module_id.to_string(),
                    globalId: m.global_id.clone(),
                    params: m.params.clone(),
                    returns: m.returns.clone(),
                }
            })
            .collect()
    }

    /// Map from entity ID to index to be used in edge references.
    fn parse_entity_map(
        sensors: &Vec<SensorJson>,
        modules: &Vec<ModuleJson>,
    ) -> HashMap<String, u32> {
        let mut map = HashMap::new();
        for sensor in sensors {
            map.insert(sensor.id.clone(), map.len() as u32);
        }
        for module in modules {
            map.insert(module.localId.clone(), map.len() as u32);
        }
        map
    }

    /// Map from tag to input entity ID and parameter index for modules.
    /// Does not include state tags.
    fn parse_tag_map(
        entity_map: &HashMap<String, u32>,
        module_jsons: &Vec<ModuleJson>,
        modules: &runner::Modules,
    ) -> HashMap<String, (u32, u32)> {
        module_jsons.iter()
            .map(|json| (&json.localId, &json.params))
            .flat_map(|(module_id, params)| {
                let id = *entity_map.get(module_id).unwrap();
                params.iter()
                    .enumerate()
                    .filter_map(move |(index, param)| {
                        modules.tags(module_id).unwrap()
                            .get_input_tag(param).unwrap().as_ref()
                            .map(|tag| (index as u32, tag.to_string()))
                    })
                    .map(move |(index, tag)| {
                        (tag, (id, index))
                    })
            })
            .collect()
    }

    fn parse_network_edges(
        entity_map: &HashMap<String, u32>,
        module_jsons: &Vec<ModuleJson>,
        modules: &runner::Modules,
    ) -> Vec<(u32, String)> {
        module_jsons.iter()
            .map(|json| &json.localId)
            .flat_map(|module_id| {
                let index = *entity_map.get(module_id).unwrap();
                let config = modules.config(module_id).unwrap();
                config.get_network_perms().clone().into_iter()
                    .map(move |domain| (index, domain))
            })
            .collect()
    }

    fn parse_intervals(
        entity_map: &HashMap<String, u32>,
        module_jsons: &Vec<ModuleJson>,
        modules: &runner::Modules,
    ) -> Vec<(u32, u32)> {
        module_jsons.iter()
            .map(|json| &json.localId)
            .filter_map(|module_id| {
                let index = *entity_map.get(module_id).unwrap();
                modules
                    .config(module_id).unwrap().get_interval()
                    .map(|duration| (index, duration))
            })
            .collect()
    }

    fn parse_data_and_state_edges(
        entity_map: &HashMap<String, u32>,
        tag_map: &HashMap<String, (u32, u32)>,
        sensor_jsons: &Vec<SensorJson>,
        module_jsons: &Vec<ModuleJson>,
        sensors: &sensors::Sensors,
        modules: &runner::Modules,
        watched_tags: &HashMap<Tag, Vec<ModuleID>>,
    ) -> (DataEdges, StateEdges) {
        let mut data_edges: DataEdges = Vec::new();
        let mut state_edges: StateEdges = Vec::new();
        let is_stateless = |tag| {
            watched_tags.get(tag)
                .map(|module_ids| !module_ids.is_empty())
                .unwrap_or(false)
        };

        for module in module_jsons {
            // For each of the module's return values, find the list of
            // output tags. If the tag is a state tag, add a state edge
            // from the return value to the sensor's state key. Otherwise,
            // find out if the edge is stateless by seeing if there is a
            // module that watches the tag. Find the input of the data
            // edge by using the entity map to map the target tags to the
            // input entity and parameter.
            let o1 = *entity_map.get(&module.localId).unwrap() as u32;
            let tags = &modules.tags(&module.localId).unwrap();
            for (o2, output) in module.returns.iter().enumerate() {
                for tag in tags.get_output_tags(output).unwrap() {
                    if state_tags::is_state_tag(&tag) {
                        let (sensor, key) = state_tags::parse_state_tag(tag);
                        let i1 = *entity_map.get(&sensor).unwrap();
                        let i2 = sensor_jsons[i1 as usize].stateKeys.iter()
                            .position(|(k,_)| k == &key).unwrap() as u32;
                        state_edges.push((o1, o2 as u32, i1, i2));
                    } else {
                        let stateless = is_stateless(tag);
                        let (i1, i2) = tag_map.get(tag).unwrap();
                        data_edges.push((stateless, o1, o2 as u32, *i1, *i2));
                    }
                }
            }
        }
        for sensor in sensor_jsons {
            let o1 = *entity_map.get(&sensor.id).unwrap() as u32;
            let tags = sensors.tags(&sensor.id).unwrap();
            for (o2, (output, _)) in sensor.returns.iter().enumerate() {
                for tag in tags.get_output_tags(output).unwrap() {
                    let stateless = is_stateless(tag);
                    let (i1, i2) = tag_map.get(tag).unwrap();
                    data_edges.push((stateless, o1, o2 as u32, *i1, *i2));
                }
            }
        }
        (data_edges, state_edges)
    }

    pub fn new(c: &Controller) -> Self {
        let sensors_lock = c.sensors.lock().unwrap();
        let modules_lock = c.modules.lock().unwrap();
        let watched_tags_lock = c.watched_tags.read().unwrap();
        let sensors = GraphJson::parse_sensors(sensors_lock.list_sensors());
        let modules = GraphJson::parse_modules(modules_lock.list_modules());
        let entity_map = GraphJson::parse_entity_map(&sensors, &modules);
        let tag_map = GraphJson::parse_tag_map(&entity_map, &modules, &modules_lock);
        let network_edges = GraphJson::parse_network_edges(&entity_map, &modules, &modules_lock);
        let intervals = GraphJson::parse_intervals(&entity_map, &modules, &modules_lock);
        let (data_edges, state_edges) = GraphJson::parse_data_and_state_edges(
            &entity_map,
            &tag_map,
            &sensors,
            &modules,
            &sensors_lock,
            &modules_lock,
            &watched_tags_lock,
        );

        GraphJson {
            sensors,
            moduleIds: modules,
            dataEdges: data_edges,
            stateEdges: state_edges,
            networkEdges: network_edges,
            intervals,
        }
    }
}

#[derive(Debug)]
pub enum Delta {
    AddModule {
        global_id: String,
        id: String,
    },
    RemoveModule {
        id: String,
    },
    AddDataEdge {
        stateless: bool,
        src_id: String,
        src_name: String,
        dst_id: String,
        dst_name: String,
    },
    RemoveDataEdge {
        stateless: bool,
        src_id: String,
        src_name: String,
        dst_id: String,
        dst_name: String,
    },
    AddStateEdge {
        src_id: String,
        src_name: String,
        dst_id: String,
        dst_name: String,
    },
    RemoveStateEdge {
        src_id: String,
        src_name: String,
        dst_id: String,
        dst_name: String,
    },
    SetNetworkEdges {
        id: String,
        domains: Vec<String>,
    },
    SetInterval {
        id: String,
        duration: Option<u32>,
    },
}

fn err(string: &str) -> Error {
    Error::BadRequestInfo(string.to_string())
}

#[derive(Default)]
struct IndexedGraphJson<'a> {
    sensors: HashSet<&'a SensorJson>,
    modules: HashSet<&'a ModuleJson>,
    data_edges_src: HashMap<u32, HashSet<(bool, String, String, String, String)>>,
    data_edges_dst: HashMap<u32, HashSet<(bool, String, String, String, String)>>,
    state_edges_src: HashMap<u32, HashSet<(String, String, String, String)>>,
    state_edges_dst: HashMap<u32, HashSet<(String, String, String, String)>>,
    network_edges: HashMap<u32, HashSet<String>>,
    intervals: HashMap<u32, Option<u32>>,
}

impl<'a> IndexedGraphJson<'a> {
    fn parse_reverse_entity_map(
        sensors: &Vec<SensorJson>,
        modules: &Vec<ModuleJson>,
    ) -> HashMap<u32, (String, Vec<String>, Vec<String>)> {
        let mut map = HashMap::new();
        for sensor in sensors {
            let index = map.len() as u32;
            let inputs = sensor.stateKeys.iter().map(|(x, _)| x.clone()).collect::<Vec<_>>();
            let outputs = sensor.returns.iter().map(|(x, _)| x.clone()).collect::<Vec<_>>();
            map.insert(index, (sensor.id.clone(), inputs, outputs));
        }
        for module in modules {
            let index = map.len() as u32;
            let inputs = module.params.clone();
            let outputs = module.returns.clone();
            map.insert(index, (module.localId.clone(), inputs, outputs));
        }
        map
    }

    fn new(graph: &'a GraphJson) -> Result<Self, Error> {
        let mut g = IndexedGraphJson::default();
        let map = Self::parse_reverse_entity_map(&graph.sensors, &graph.moduleIds);
        g.sensors = graph.sensors.iter().collect();
        g.modules = graph.moduleIds.iter().collect();
        for i in 0..graph.sensors.len() {
            let i = i as u32;
            g.data_edges_src.insert(i, HashSet::new());
            g.state_edges_dst.insert(i, HashSet::new());
        }
        for i in 0..graph.moduleIds.len() {
            let i = (graph.sensors.len() + i) as u32;
            g.data_edges_src.insert(i, HashSet::new());
            g.data_edges_dst.insert(i, HashSet::new());
            g.state_edges_src.insert(i, HashSet::new());
            g.state_edges_dst.insert(i, HashSet::new());
            g.network_edges.insert(i, HashSet::new());
            g.intervals.insert(i, None);
        }
        for (stateless, a, b, c, d) in &graph.dataEdges {
            let (src_id, _, outputs) = map.get(a).ok_or(err("data edge src id"))?;
            let src_name = outputs.get(*b as usize).ok_or(err("data edge src name"))?.to_string();
            let (dst_id, inputs, _) = map.get(c).ok_or(err("data edge dst id"))?;
            let dst_name = inputs.get(*d as usize).ok_or(err("data edge dst name"))?.to_string();
            let edge = (*stateless, src_id.to_string(), src_name, dst_id.to_string(), dst_name);
            g.data_edges_src.get_mut(a).unwrap().insert(edge.clone());
            g.data_edges_dst.get_mut(c).unwrap().insert(edge);
        }
        for (a, b, c, d) in &graph.stateEdges {
            let (src_id, _, outputs) = map.get(a).ok_or(err("state edge src id"))?;
            let src_name = outputs.get(*b as usize).ok_or(err("state edge src name"))?.to_string();
            let (dst_id, inputs, _) = map.get(c).ok_or(err("state edge dst id"))?;
            let dst_name = inputs.get(*d as usize).ok_or(err("state edge dst name"))?.to_string();
            let edge = (src_id.to_string(), src_name, dst_id.to_string(), dst_name);
            g.state_edges_src.get_mut(a).unwrap().insert(edge.clone());
            g.state_edges_dst.get_mut(c).unwrap().insert(edge);
        }
        for (a, domain) in &graph.networkEdges {
            g.network_edges.get_mut(a).ok_or(err("network edge module id"))?.insert(domain.clone());
        }
        for (a, duration) in &graph.intervals {
            *g.intervals.get_mut(a).ok_or(err("interval module id"))? = Some(*duration);
        }
        Ok(g)
    }
}

impl GraphJson {
    /// Calculate the delta needed to change the current graph to the new one.
    /// All the sensors must be the same.
    pub fn calculate_delta(&self, new: &GraphJson) -> Result<Vec<Delta>, Error> {
        let mut g1 = IndexedGraphJson::new(self)?;
        let mut g2 = IndexedGraphJson::new(new)?;
        if g1.sensors != g2.sensors {
            return Err(err("sensors do not match"));
        }

        // Calculate which modules to remove, keep, and add.
        let mut modules_to_remove: Vec<ModuleID> = Vec::new();
        let mut modules_to_keep: Vec<ModuleID> = Vec::new();
        let mut modules_to_add: Vec<(ModuleID, GlobalModuleID)> = Vec::new();
        for m in g1.modules {
            if g2.modules.remove(&m) {
                modules_to_keep.push(m.localId.clone());
            } else {
                modules_to_remove.push(m.localId.clone());
            }
        }
        for m in g2.modules {
            modules_to_add.push((m.localId.clone(), m.globalId.clone()));
        }

        let mut deltas = vec![];
        let old_entity_map = Self::parse_entity_map(&self.sensors, &self.moduleIds);
        let new_entity_map = Self::parse_entity_map(&new.sensors, &new.moduleIds);

        // Remove all edges connected to the removed modules.
        // Then remove the modules.
        // The following data structure ensures edges are not removed twice
        // when later removing data edges where the source is a sensor.
        let mut removed_data_edges_dst = HashSet::new();
        for id in modules_to_remove {
            let i = old_entity_map.get(&id).unwrap();
            let domains = g1.network_edges.remove(i).unwrap();
            if !domains.is_empty() {
                deltas.push(Delta::SetNetworkEdges {
                    id: id.clone(),
                    domains: vec![],
                });
            }
            let duration = g1.intervals.remove(i).unwrap();
            if duration.is_some() {
                deltas.push(Delta::SetInterval {
                    id: id.clone(),
                    duration: None,
                });
            }
            for (stateless, a, b, c, d) in g1.data_edges_src.remove(i).unwrap() {
                deltas.push(Delta::RemoveDataEdge {
                    stateless,
                    src_id: a.clone(),
                    src_name: b.clone(),
                    dst_id: c.clone(),
                    dst_name: d.clone(),
                });
            }
            for (stateless, a, b, c, d) in g1.data_edges_dst.remove(i).unwrap() {
                deltas.push(Delta::RemoveDataEdge {
                    stateless,
                    src_id: a.clone(),
                    src_name: b.clone(),
                    dst_id: c.clone(),
                    dst_name: d.clone(),
                });
                removed_data_edges_dst.insert((stateless, a, b, c, d));
            }
            for (a, b, c, d) in g1.state_edges_src.remove(i).unwrap() {
                deltas.push(Delta::RemoveStateEdge {
                    src_id: a.clone(),
                    src_name: b.clone(),
                    dst_id: c.clone(),
                    dst_name: d.clone(),
                });
            }
            deltas.push(Delta::RemoveModule { id });
        }

        // Add new modules.
        for (id, global_id) in modules_to_add.clone() {
            deltas.push(Delta::AddModule {id, global_id });
        }

        // For all remaining old modules:
        // set the network edges (if changed)
        // set the intervals (if changed)
        // set outgoing data edges (if changed)
        // set outgoing state edges (if changed)
        for id in modules_to_keep {
            let i1 = old_entity_map.get(&id).unwrap();
            let i2 = new_entity_map.get(&id).unwrap();
            let network_edges1 = g1.network_edges.remove(i1).unwrap();
            let network_edges2 = g2.network_edges.remove(i2).unwrap();
            if network_edges1 != network_edges2 {
                deltas.push(Delta::SetNetworkEdges {
                    id: id.clone(),
                    domains: network_edges2.into_iter().collect(),
                });
            }
            let intervals1 = g1.intervals.remove(i1).unwrap();
            let intervals2 = g2.intervals.remove(i2).unwrap();
            if intervals1 != intervals2 {
                deltas.push(Delta::SetInterval { id, duration: intervals2 });
            }
            let data_edges1 = g1.data_edges_src.remove(i1).unwrap();
            let mut data_edges2 = g2.data_edges_src.remove(i2).unwrap();
            for edge in data_edges1 {
                if !data_edges2.remove(&edge) {
                    deltas.push(Delta::RemoveDataEdge {
                        stateless: edge.0,
                        src_id: edge.1,
                        src_name: edge.2,
                        dst_id: edge.3,
                        dst_name: edge.4,
                    });
                }
            }
            for edge in data_edges2 {
                deltas.push(Delta::AddDataEdge {
                    stateless: edge.0,
                    src_id: edge.1,
                    src_name: edge.2,
                    dst_id: edge.3,
                    dst_name: edge.4,
                });
            }
            let state_edges1 = g1.state_edges_src.remove(i1).unwrap();
            let mut state_edges2 = g2.state_edges_src.remove(i2).unwrap();
            for edge in state_edges1 {
                if !state_edges2.remove(&edge) {
                    deltas.push(Delta::RemoveStateEdge {
                        src_id: edge.0,
                        src_name: edge.1,
                        dst_id: edge.2,
                        dst_name: edge.3,
                    });
                }
            }
            for edge in state_edges2 {
                deltas.push(Delta::AddStateEdge {
                    src_id: edge.0,
                    src_name: edge.1,
                    dst_id: edge.2,
                    dst_name: edge.3,
                });
            }
        }

        // For sensors: remove the necessary data edges that have not already
        // been removed, and add new data edges.
        for id in g1.sensors.iter().map(|s| &s.id) {
            let i1 = old_entity_map.get(id).unwrap();
            let i2 = new_entity_map.get(id).unwrap();
            let data_edges1 = g1.data_edges_src.remove(i1).unwrap();
            let mut data_edges2 = g2.data_edges_src.remove(i2).unwrap();
            for edge in data_edges1 {
                if !data_edges2.remove(&edge) {
                    if !removed_data_edges_dst.contains(&edge) {
                        deltas.push(Delta::RemoveDataEdge {
                            stateless: edge.0,
                            src_id: edge.1,
                            src_name: edge.2,
                            dst_id: edge.3,
                            dst_name: edge.4,
                        });
                    }
                }
            }
            for edge in data_edges2 {
                deltas.push(Delta::AddDataEdge {
                    stateless: edge.0,
                    src_id: edge.1,
                    src_name: edge.2,
                    dst_id: edge.3,
                    dst_name: edge.4,
                });
            }
        }

        // For all completely new modules:
        // set the network edges
        // set the intervals
        // set outgoing data edges
        // set outgoing state edges
        for (id, _) in modules_to_add {
            let i = new_entity_map.get(&id).unwrap();
            let domains = g2.network_edges.remove(i).unwrap();
            if !domains.is_empty() {
                deltas.push(Delta::SetNetworkEdges {
                    id: id.clone(),
                    domains: domains.into_iter().collect(),
                });
            }
            let duration = g2.intervals.remove(i).unwrap();
            if duration.is_some() {
                deltas.push(Delta::SetInterval {
                    id,
                    duration,
                });
            }
            for edge in g2.data_edges_src.remove(i).unwrap() {
                deltas.push(Delta::AddDataEdge {
                    stateless: edge.0,
                    src_id: edge.1,
                    src_name: edge.2,
                    dst_id: edge.3,
                    dst_name: edge.4,
                });
            }
            for edge in g2.state_edges_src.remove(i).unwrap() {
                deltas.push(Delta::AddStateEdge {
                    src_id: edge.0,
                    src_name: edge.1,
                    dst_id: edge.2,
                    dst_name: edge.3,
                });
            }
        }
        debug!("{:?}", deltas);
        Ok(deltas)
    }
}
