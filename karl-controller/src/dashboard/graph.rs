use std::collections::{HashSet, HashMap};
use karl_common::*;
use crate::controller::{sensors, runner, Controller};

#[allow(non_snake_case)]
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct GraphJson {
    // The number of initial nodes that are devices
    pub n_devices: usize,
    // sensors indexed 0 to n-1, where n is the number of sensors
    pub nodes: Vec<NodeJson>,
    // stateless, out_id, out_red, module_id, module_param
    pub dataEdges: DataEdges,
    // module_id, module_ret, sensor_id, sensor_key
    pub stateEdges: StateEdges,
    // module_id, domain
    pub networkEdges: Vec<(u32, String)>,
    // module_id, duration_s
    pub intervals: Vec<(u32, u32)>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct PolicyJson {
    // pipeline, allowed
    pub pipelines: Vec<(String, bool)>,
    // tag, context
    // - tag: <node>.<value> or #<device>.<input>
    // - context: PRIVATE, PUBLIC, <module>
    pub contexts: Vec<(String, String)>,
}

#[allow(non_snake_case)]
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct NodeJson {
    pub id: String,
    pub globalId: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

type DataEdges = Vec<(bool, u32, u32, u32, u32)>;
type StateEdges = Vec<(u32, u32, u32, u32)>;

impl GraphJson {
    fn parse_nodes(
        sensors: Vec<&sensors::Sensor>,
        modules: Vec<(&ModuleID, &Module)>,
    ) -> Vec<NodeJson> {
        // TODO: sort sensors?
        let mut nodes: Vec<NodeJson> = sensors.iter()
            .filter(|s| s.confirmed)
            .map(|s| {
                NodeJson {
                    id: s.id.clone(),
                    globalId: s.id.clone(),
                    inputs: s.keys.clone(),
                    outputs: s.returns.clone(),
                }
            })
            .collect();
        let mut modules = modules.iter()
            .map(|(module_id, m)| {
                NodeJson {
                    id: module_id.to_string(),
                    globalId: m.global_id.clone(),
                    inputs: m.params.clone(),
                    outputs: m.returns.clone(),
                }
            })
            .collect();
        nodes.append(&mut modules);
        nodes
    }

    /// Map from entity ID to index to be used in edge references.
    fn parse_entity_map(
        nodes: &Vec<NodeJson>,
    ) -> HashMap<String, u32> {
        nodes.iter()
            .enumerate()
            .map(|(i, node)| (node.id.clone(), i as u32))
            .collect()
    }

    /// Map from tag to input entity ID and parameter index for modules.
    /// Does not include state tags.
    fn parse_tag_map(
        n_devices: usize,
        entity_map: &HashMap<String, u32>,
        node_jsons: &Vec<NodeJson>,
    ) -> HashMap<String, (u32, u32)> {
        node_jsons[n_devices..]
            .iter()
            .map(|json| (&json.id, &json.inputs))
            .flat_map(|(module_id, params)| {
                let id = *entity_map.get(module_id).unwrap();
                params.iter()
                    .enumerate()
                    .map(move |(index, input)| {
                        let tag = format!("{}.{}", module_id, input);
                        (tag, (id, index as u32))
                    })
            })
            .collect()
    }

    fn parse_network_edges(
        n_devices: usize,
        entity_map: &HashMap<String, u32>,
        node_jsons: &Vec<NodeJson>,
        modules: &runner::Modules,
    ) -> Vec<(u32, String)> {
        node_jsons[n_devices..]
            .iter()
            .map(|json| &json.id)
            .flat_map(|module_id| {
                let index = *entity_map.get(module_id).unwrap();
                let config = modules.config(module_id).unwrap();
                config.get_network_perms().clone().into_iter()
                    .map(move |domain| (index, domain))
            })
            .collect()
    }

    fn parse_intervals(
        n_devices: usize,
        entity_map: &HashMap<String, u32>,
        node_jsons: &Vec<NodeJson>,
        modules: &runner::Modules,
    ) -> Vec<(u32, u32)> {
        node_jsons[n_devices..]
            .iter()
            .map(|json| &json.id)
            .filter_map(|module_id| {
                let index = *entity_map.get(module_id).unwrap();
                modules
                    .config(module_id).unwrap().get_interval()
                    .map(|duration| (index, duration))
            })
            .collect()
    }

    fn parse_data_and_state_edges(
        n_devices: usize,
        entity_map: &HashMap<String, u32>,
        tag_map: &HashMap<String, (u32, u32)>,
        node_jsons: &Vec<NodeJson>,
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

        for module in &node_jsons[n_devices..] {
            // For each of the module's return values, find the list of
            // output tags. If the tag is a state tag, add a state edge
            // from the return value to the sensor's state key. Otherwise,
            // find out if the edge is stateless by seeing if there is a
            // module that watches the tag. Find the input of the data
            // edge by using the entity map to map the target tags to the
            // input entity and parameter.
            let o1 = *entity_map.get(&module.id).unwrap() as u32;
            let tags = &modules.tags(&module.id).unwrap();
            for (o2, output) in module.outputs.iter().enumerate() {
                for tag in tags.get_output_tags(output).unwrap() {
                    if tag_parsing::is_state_tag(&tag) {
                        let (sensor, key) = tag_parsing::parse_state_tag(tag);
                        let i1 = *entity_map.get(&sensor).unwrap();
                        let i2 = node_jsons[i1 as usize].inputs.iter()
                            .position(|k| k == &key).unwrap() as u32;
                        state_edges.push((o1, o2 as u32, i1, i2));
                    } else {
                        let stateless = is_stateless(tag);
                        let (i1, i2) = tag_map.get(tag).unwrap();
                        data_edges.push((stateless, o1, o2 as u32, *i1, *i2));
                    }
                }
            }
        }
        for sensor in &node_jsons[..n_devices] {
            let o1 = *entity_map.get(&sensor.id).unwrap() as u32;
            let tags = sensors.tags(&sensor.id).unwrap();
            for (o2, output) in sensor.outputs.iter().enumerate() {
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
        let modules_lock = c.modules.read().unwrap();
        let watched_tags_lock = c.watched_tags.read().unwrap();
        let (n_devices, nodes) = {
            let sensors = sensors_lock.list_sensors();
            let modules = modules_lock.list_modules();
            let n_devices = sensors.len();
            let nodes = GraphJson::parse_nodes(sensors, modules);
            (n_devices, nodes)
        };
        let entity_map = GraphJson::parse_entity_map(&nodes);
        let tag_map = GraphJson::parse_tag_map(n_devices, &entity_map, &nodes);
        let network_edges = GraphJson::parse_network_edges(n_devices, &entity_map, &nodes, &modules_lock);
        let intervals = GraphJson::parse_intervals(n_devices, &entity_map, &nodes, &modules_lock);
        let (data_edges, state_edges) = GraphJson::parse_data_and_state_edges(
            n_devices,
            &entity_map,
            &tag_map,
            &nodes,
            &sensors_lock,
            &modules_lock,
            &watched_tags_lock,
        );

        GraphJson {
            n_devices,
            nodes,
            dataEdges: data_edges,
            stateEdges: state_edges,
            networkEdges: network_edges,
            intervals,
        }
    }
}

impl PolicyJson {
    pub fn new(c: &Controller) -> Self {
        let policies = c.policies.read().unwrap();
        Self {
            pipelines: policies.get_pipeline_strings(),
            contexts: policies.get_security_context_strings(),
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
    sensors: HashSet<&'a NodeJson>,
    modules: HashSet<&'a NodeJson>,
    data_edges_src: HashMap<u32, HashSet<(bool, String, String, String, String)>>,
    data_edges_dst: HashMap<u32, HashSet<(bool, String, String, String, String)>>,
    state_edges_src: HashMap<u32, HashSet<(String, String, String, String)>>,
    state_edges_dst: HashMap<u32, HashSet<(String, String, String, String)>>,
    network_edges: HashMap<u32, HashSet<String>>,
    intervals: HashMap<u32, Option<u32>>,
}

impl<'a> IndexedGraphJson<'a> {
    fn parse_reverse_entity_map(
        nodes: &Vec<NodeJson>,
    ) -> HashMap<u32, (String, Vec<String>, Vec<String>)> {
        let mut map = HashMap::new();
        for (index, node) in nodes.iter().enumerate() {
            let inputs = node.inputs.clone();
            let outputs = node.outputs.clone();
            map.insert(index as u32, (node.id.clone(), inputs, outputs));
        }
        map
    }

    fn new(graph: &'a GraphJson) -> Result<Self, Error> {
        let mut g = IndexedGraphJson::default();
        let map = Self::parse_reverse_entity_map(&graph.nodes);
        g.sensors = graph.nodes[..graph.n_devices].iter().collect();
        g.modules = graph.nodes[graph.n_devices..].iter().collect();
        for i in 0..graph.n_devices {
            let i = i as u32;
            g.data_edges_src.insert(i, HashSet::new());
            g.state_edges_dst.insert(i, HashSet::new());
        }
        for i in graph.n_devices..graph.nodes.len() {
            let i = i as u32;
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
                modules_to_keep.push(m.id.clone());
            } else {
                modules_to_remove.push(m.id.clone());
            }
        }
        for m in g2.modules {
            modules_to_add.push((m.id.clone(), m.globalId.clone()));
        }

        let mut deltas = vec![];
        let old_entity_map = Self::parse_entity_map(&self.nodes);
        let new_entity_map = Self::parse_entity_map(&new.nodes);

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
