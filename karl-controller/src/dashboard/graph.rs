use std::collections::HashMap;
use karl_common::*;
use crate::controller::{sensors, runner, tags::*, Controller};

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
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SensorJson {
    pub id: String,
    pub stateKeys: Vec<(String, String)>,
    pub returns: Vec<(String, String)>,
}

#[allow(non_snake_case)]
#[derive(Debug, Default, Serialize, Deserialize)]
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
        modules: &Vec<ModuleJson>,
    ) -> HashMap<String, (u32, u32)> {
        modules.iter()
            .flat_map(|m| {
                let id = *entity_map.get(&m.localId).unwrap();
                m.params.iter().enumerate().map(move |(index, tag)| {
                    (tag.to_string(), (id, index as u32))
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
                    if is_state_tag(&tag) {
                        let (sensor, key) = parse_state_tag(tag);
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
        let tag_map = GraphJson::parse_tag_map(&entity_map, &modules);
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
