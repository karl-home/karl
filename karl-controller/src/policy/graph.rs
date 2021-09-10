//! Dataflow graph representing privacy policies.
use std::fmt;
use std::collections::HashMap;
use crate::{SensorJson, ModuleJson, GraphJson};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PolicyGraph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Number of devices (the first `n_devices` nodes are devices)
    pub n_devices: usize,
    /// Indices of nodes with network access and the domain names
    pub network_nodes: HashMap<usize, Vec<String>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Edge {
    pub src_node: usize,
    pub src_index: usize,
    pub dst_node: usize,
    pub dst_index: usize,
}

impl From<&GraphJson> for PolicyGraph {
    fn from(json: &GraphJson) -> Self {
        let mut graph = PolicyGraph::default();
        graph.n_devices = json.sensors.len();
        for sensor in &json.sensors {
            graph.nodes.push(Node {
                id: sensor.id.clone(),
                inputs: sensor.stateKeys.iter().map(|(input, _)| input.clone()).collect(),
                outputs: sensor.returns.iter().map(|(output, _)| output.clone()).collect(),
            });
        }
        for module in &json.moduleIds {
            graph.nodes.push(Node {
                id: module.localId.clone(),
                inputs: module.params.clone(),
                outputs: module.returns.clone(),
            });
        }
        for (_, src_node, src_index, dst_node, dst_index) in &json.dataEdges {
            graph.edges.push(Edge {
                src_node: *src_node as usize,
                src_index: *src_index as usize,
                dst_node: *dst_node as usize,
                dst_index: *dst_index as usize,
            })
        }
        for (src_node, src_index, dst_node, dst_index) in &json.stateEdges {
            graph.edges.push(Edge {
                src_node: *src_node as usize,
                src_index: *src_index as usize,
                dst_node: *dst_node as usize,
                dst_index: *dst_index as usize,
            })
        }
        for (node, domain) in &json.networkEdges {
            graph.network_nodes
                .entry(*node as usize)
                .or_insert(Vec::new())
                .push(domain.clone());
        }
        graph
    }
}

impl fmt::Display for PolicyGraph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in 0..self.n_devices {
            writeln!(f, "{}*", &self.nodes[i].id)?;
        }
        for i in self.n_devices..self.nodes.len() {
            if let Some(domains) = self.network_nodes.get(&i) {
                writeln!(f, "{} {:?}", &self.nodes[i].id, domains)?;
            } else {
                writeln!(f, "{}", &self.nodes[i].id)?;
            }
        }
        writeln!(f, "")?;
        for edge in &self.edges {
            writeln!(
                f,
                "{}.{} -> {}.{}",
                self.nodes[edge.src_node].id,
                self.nodes[edge.src_node].outputs[edge.src_index],
                self.nodes[edge.dst_node].id,
                self.nodes[edge.dst_node].inputs[edge.dst_index],
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn sensor_json(
        id: &str,
        keys: Vec<&str>,
        returns: Vec<&str>,
    ) -> SensorJson {
        SensorJson {
            id: id.to_string(),
            stateKeys: keys.into_iter()
                .map(|x| (x.to_string(), String::from("")))
                .collect(),
            returns: returns.into_iter()
                .map(|x| (x.to_string(), String::from("")))
                .collect(),
        }
    }

    fn module_json(
        id: &str,
        params: Vec<&str>,
        returns: Vec<&str>,
    ) -> ModuleJson {
        ModuleJson {
            localId: id.to_string(),
            globalId: id.to_string(),
            params: params.into_iter().map(|x| x.to_string()).collect(),
            returns: returns.into_iter().map(|x| x.to_string()).collect(),
        }
    }

    fn pipeline_i() -> GraphJson {
        GraphJson {
            sensors: vec![
                sensor_json("camera", vec!["firmware", "livestream"], vec!["motion", "streaming"]),
            ],
            moduleIds: vec![
                module_json("set_true", vec![], vec!["true"]),
            ],
            dataEdges: vec![],
            stateEdges: vec![(1, 0, 0, 1)],
            networkEdges: vec![],
            intervals: vec![],
        }
    }

    fn pipeline_ii() -> GraphJson {
        GraphJson {
            sensors: vec![
                sensor_json("speaker", vec!["playback"], vec!["speech_command"]),
            ],
            moduleIds: vec![
                module_json("picovoice", vec!["speech"], vec!["weather_intent", "light_intent"]),
                module_json("weather", vec!["weather_intent"], vec!["weather"]),
            ],
            dataEdges: vec![(true, 0, 0, 1, 0), (true, 1, 0, 2, 0)],
            stateEdges: vec![(2, 0, 0, 0)],
            networkEdges: vec![(2, "weather.com".to_string())],
            intervals: vec![],
        }
    }

    fn pipeline_iii() -> GraphJson {
        GraphJson {
            sensors: vec![
                sensor_json("speaker", vec!["playback"], vec!["speech_command"]),
                sensor_json("light", vec!["state", "intensity"], vec!["state", "intensity"])
            ],
            moduleIds: vec![
                module_json("picovoice", vec!["speech"], vec!["weather_intent", "light_intent"]),
                module_json("light_switch", vec!["light_intent"], vec!["state"]),
            ],
            dataEdges: vec![(true, 0, 0, 2, 0), (true, 2, 1, 3, 0)],
            stateEdges: vec![(3, 0, 1, 0)],
            networkEdges: vec![],
            intervals: vec![],
        }
    }

    fn pipeline_iv() -> GraphJson {
        GraphJson {
            sensors: vec![
                sensor_json("camera", vec!["firmware", "livestream"], vec!["motion", "streaming"]),
                sensor_json("occupancy_sensor", vec![], vec!["at_home"]),
            ],
            moduleIds: vec![
                module_json("person_detection", vec!["image"], vec!["training_data", "count"]),
                module_json("boolean", vec!["condition", "value"], vec!["predicate"]),
                module_json("statistics", vec!["data"], vec![]),
            ],
            dataEdges: vec![(true, 0, 0, 2, 0), (false, 1, 0, 3, 0), (true, 2, 0, 3, 1), (true, 3, 0, 4, 0)],
            stateEdges: vec![],
            networkEdges: vec![(4, "statistics.com".to_string())],
            intervals: vec![],
        }
    }

    #[test]
    fn test_pipeline_i_parsing() {
        let json = pipeline_i();
        let graph = PolicyGraph::from(&json);
        assert_eq!(graph.nodes.len(), json.sensors.len() + json.moduleIds.len());
        assert_eq!(graph.edges.len(), json.dataEdges.len() + json.stateEdges.len());
        assert_eq!(graph.n_devices, json.sensors.len());
        assert_eq!(graph.network_nodes.len(), json.networkEdges.len());
        println!("{}", graph);
    }

    #[test]
    fn test_pipeline_ii_parsing() {
        let json = pipeline_ii();
        let graph = PolicyGraph::from(&json);
        assert_eq!(graph.nodes.len(), json.sensors.len() + json.moduleIds.len());
        assert_eq!(graph.edges.len(), json.dataEdges.len() + json.stateEdges.len());
        assert_eq!(graph.n_devices, json.sensors.len());
        assert_eq!(graph.network_nodes.len(), json.networkEdges.len());
        println!("{}", graph);
    }

    #[test]
    fn test_pipeline_iii_parsing() {
        let json = pipeline_iii();
        let graph = PolicyGraph::from(&json);
        assert_eq!(graph.nodes.len(), json.sensors.len() + json.moduleIds.len());
        assert_eq!(graph.edges.len(), json.dataEdges.len() + json.stateEdges.len());
        assert_eq!(graph.n_devices, json.sensors.len());
        assert_eq!(graph.network_nodes.len(), json.networkEdges.len());
        println!("{}", graph);
    }

    #[test]
    fn test_pipeline_iv_parsing() {
        let json = pipeline_iv();
        let graph = PolicyGraph::from(&json);
        assert_eq!(graph.nodes.len(), json.sensors.len() + json.moduleIds.len());
        assert_eq!(graph.edges.len(), json.dataEdges.len() + json.stateEdges.len());
        assert_eq!(graph.n_devices, json.sensors.len());
        assert_eq!(graph.network_nodes.len(), json.networkEdges.len());
        println!("{}", graph);
    }
}
