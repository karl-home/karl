//! Dataflow graph representing privacy policies.
use std::fmt;
use std::collections::{HashMap, VecDeque};
use crate::GraphJson;

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct PolicyGraph {
    pub nodes: Vec<Node>,
    /// Map from <node_id> to node index
    pub(crate) node_map: HashMap<String, usize>,
    /// Map from <node_id>.<value> to node index, input/output index, is_input
    pub(crate) tag_map: HashMap<String, (usize, usize, bool)>,
    pub edges: HashMap<EdgeNode, Vec<EdgeNode>>,
    /// Number of devices (the first `n_devices` nodes are devices)
    pub n_devices: usize,
    /// Indices of nodes with network access and the domain names
    pub network_nodes: HashMap<usize, Vec<String>>,
    /// Pipelines so the indexes are maintained
    pub pipelines: Vec<Pipeline>
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Node {
    pub id: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct EdgeNode {
    pub node: usize,
    pub index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PipelineNode {
    Data { device: usize, output: usize },
    ModuleInput { module: usize, index: usize },
    ModuleOutput { module: usize, index: usize },
    Network { domain: String },
    Actuator {device: usize, input: usize },
}

impl PipelineNode {
    pub fn is_module_input(&self) -> bool {
        match self {
            PipelineNode::ModuleInput { .. } => true,
            _ => false,
        }
    }

    pub fn is_module_output(&self) -> bool {
        match self {
            PipelineNode::ModuleOutput { .. } => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub source: PipelineNode,
    pub nodes: Vec<PipelineNode>,
}

impl EdgeNode {
    pub fn new(node: &u32, index: &u32) -> Self {
        Self {
            node: *node as usize,
            index: *index as usize,
        }
    }
}

impl Pipeline {
    pub fn new(source: PipelineNode) -> Pipeline {
        Self {
            source,
            nodes: Vec::new(),
        }
    }

    pub fn get_source(&self) -> &PipelineNode {
        &self.source
    }

    pub fn get_sink(&self) -> &PipelineNode {
        if self.nodes.is_empty() {
            &self.source
        } else {
            &self.nodes[self.nodes.len() - 1]
        }
    }

    pub fn push_node(&mut self, node: PipelineNode) {
        self.nodes.push(node);
    }

    pub fn len(&self) -> usize {
        self.nodes.len() + 1
    }

    pub fn is_sensitive(&self) -> bool {
        if !self.nodes.is_empty() {
            let sink = self.get_sink();
            match self.get_source() {
                PipelineNode::Data { device: device_source , .. } => match sink {
                    PipelineNode::Network {..} => true,
                    PipelineNode::Actuator { device: device_sink, .. } => {
                        device_source != device_sink
                    },
                    _ => false,
                },
                PipelineNode::Network {..} => match sink {
                    PipelineNode::Actuator {..}  => true,
                    _ => false,
                },
                _ => false,
            }
        } else {
            false
        }
    }

    pub fn contains_node(&self, node: &PipelineNode) -> bool {
        &self.source == node || self.nodes.contains(node)
    }
}

impl PolicyGraph {
    /// Calculate all pipeline policies from the graph.
    /// - device output to network sink
    /// - network source to device input
    /// - device output to other device's input
    fn calculate_pipelines(&self) -> Vec<Pipeline> {
        // Assemble all possible sources
        let mut queue: VecDeque<Pipeline> = VecDeque::new();
        for device_i in 0..self.n_devices {
            let device = &self.nodes[device_i];
            for output_i in 0..device.outputs.len() {
                // Source can be device data
                let source = PipelineNode::Data {
                    device: device_i,
                    output: output_i,
                };
                let pipeline = Pipeline::new(source);
                queue.push_back(pipeline);
            }
        }
        for (module_i, domains) in self.network_nodes.iter() {
            for domain in domains {
                for output_i in 0..(self.nodes[*module_i].outputs.len()) {
                    let source = PipelineNode::Network { domain: domain.clone() };
                    let mut pipeline = Pipeline::new(source);
                    pipeline.push_node(PipelineNode::ModuleOutput {
                        module: *module_i,
                        index: output_i,
                    });
                    queue.push_back(pipeline);
                }
            }
        }

        // BFS from device outputs and network sources
        let mut sensitive_pipelines = Vec::new();
        while let Some(pipeline) = queue.pop_front() {
            let (node, output) = match pipeline.get_sink() {
                PipelineNode::Data { device, output } => {
                    // Find edges to module inputs only. Since sensitive
                    // dataflow can't end in a module input, also add edges
                    // from module inputs to module outputs and networks.
                    (*device, *output)
                },
                PipelineNode::ModuleOutput { module, index: output } => {
                    // This can only be a module _output_ due to how we
                    // handle PipelineNode::Data. Find edges to module
                    // inputs (which should be treated the same as above)
                    // or actuators.
                    (*module, *output)
                },
                PipelineNode::ModuleInput { .. } => { unreachable!() },
                PipelineNode::Network { .. } | PipelineNode::Actuator { .. } => {
                    // Network pipeline nodes don't have edges to anything,
                    // unless it is a source. Actuators don't have edges to
                    // anything.
                    if pipeline.is_sensitive() {
                        sensitive_pipelines.push(pipeline);
                    }
                    continue;
                },
            };

            let sink = EdgeNode { node, index: output };
            let inputs = if let Some(inputs) = self.edges.get(&sink) {
                inputs
            } else {
                continue;
            };
            for input in inputs {
                // If the edge is to a device...
                if input.node < self.n_devices {
                    let mut pipeline = pipeline.clone();
                    pipeline.push_node(PipelineNode::Actuator {
                        device: input.node,
                        input: input.index,
                    });
                    queue.push_back(pipeline);
                    continue;
                }

                // Otherwise it is to a module.
                for output_i in 0..self.nodes[input.node].outputs.len() {
                    // The output might be a module
                    let mut pipeline = pipeline.clone();
                    pipeline.push_node(PipelineNode::ModuleInput {
                        module: input.node,
                        index: input.index,
                    });
                    pipeline.push_node(PipelineNode::ModuleOutput {
                        module: input.node,
                        index: output_i,
                    });
                    queue.push_back(pipeline);
                }
                if let Some(domains) = self.network_nodes.get(&input.node) {
                    for domain in domains {
                        let mut pipeline = pipeline.clone();
                        pipeline.push_node(PipelineNode::ModuleInput {
                            module: input.node,
                            index: input.index,
                        });
                        pipeline.push_node(PipelineNode::Network {
                            domain: domain.clone(),
                        });
                        queue.push_back(pipeline);
                    }
                }
            }
        }
        sensitive_pipelines
    }

    pub fn pnode_to_string(&self, node: PipelineNode) -> String {
        match node {
            PipelineNode::Data { device, output } => format!(
                "*{}.{}",
                self.nodes[device].id,
                self.nodes[device].outputs[output],
            ),
            PipelineNode::ModuleInput { module, .. } => {
                self.nodes[module].id.clone()
            },
            PipelineNode::ModuleOutput { module, .. } => {
                self.nodes[module].id.clone()
            },
            PipelineNode::Network { domain } => domain,
            PipelineNode::Actuator { device, input } => format!(
                "#{}.{}",
                self.nodes[device].id,
                self.nodes[device].inputs[input],
            ),
        }
    }

    /// Remove the given pipelines, returning a new graph.
    /// Each value in the array is an index into a cached pipeline.
    /// Thus each value must be less than self.pipelines.len()
    pub fn apply_removed_pipelines(&self, removed: Vec<usize>) -> PolicyGraph {
        for i in removed {
            assert!(i < self.pipelines.len());
        }
        self.clone()
        //unimplemented!()
    }
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
        for (i, node) in graph.nodes.iter().enumerate() {
            graph.node_map.insert(node.id.clone(), i);
            for (input_i, input) in node.inputs.iter().enumerate() {
                let tag = if i < graph.n_devices {
                    format!("#{}.{}", node.id, input)
                } else {
                    format!("{}.{}", node.id, input)
                };
                graph.tag_map.insert(tag, (i, input_i, true));
            }
            for (output_i, output) in node.outputs.iter().enumerate() {
                graph.tag_map.insert(
                    format!("{}.{}", node.id, output),
                    (i, output_i, false),
                );
            }
        }

        for (_, src_node, src_index, dst_node, dst_index) in &json.dataEdges {
            graph.edges
                .entry(EdgeNode::new(src_node, src_index))
                .or_insert(Vec::new())
                .push(EdgeNode::new(dst_node, dst_index));
        }
        for (src_node, src_index, dst_node, dst_index) in &json.stateEdges {
            graph.edges
                .entry(EdgeNode::new(src_node, src_index))
                .or_insert(Vec::new())
                .push(EdgeNode::new(dst_node, dst_index));
        }
        for (node, domain) in &json.networkEdges {
            graph.network_nodes
                .entry(*node as usize)
                .or_insert(Vec::new())
                .push(domain.clone());
        }
        graph.pipelines = graph.calculate_pipelines();
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
        for (edge_src, edge_dsts) in self.edges.iter() {
            for edge_dst in edge_dsts {
                writeln!(
                    f,
                    "{}.{} -> {}.{}",
                    self.nodes[edge_src.node].id,
                    self.nodes[edge_src.node].outputs[edge_src.index],
                    self.nodes[edge_dst.node].id,
                    self.nodes[edge_dst.node].inputs[edge_dst.index],
                )?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{ModuleJson, SensorJson};

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

    fn pipeline_old_ii() -> GraphJson {
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

    fn pipeline_ii() -> GraphJson {
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

    fn pipeline_iii() -> GraphJson {
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
        println!("{}", graph);
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.n_devices, 1);
        assert_eq!(graph.network_nodes.len(), 0);
        assert_eq!(graph.pipelines.len(), 0);
    }

    #[test]
    fn test_pipeline_old_ii_parsing() {
        let json = pipeline_old_ii();
        let graph = PolicyGraph::from(&json);
        println!("{}", graph);
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 3);
        assert_eq!(graph.n_devices, 1);
        assert_eq!(graph.network_nodes.len(), 1);
        assert_eq!(graph.pipelines.len(), 2);
    }

    #[test]
    fn test_pipeline_ii_parsing() {
        let json = pipeline_ii();
        let graph = PolicyGraph::from(&json);
        assert_eq!(graph.nodes.len(), 4);
        assert_eq!(graph.edges.len(), 3);
        assert_eq!(graph.n_devices, 2);
        assert_eq!(graph.network_nodes.len(), 0);
        assert_eq!(graph.pipelines.len(), 1);
    }

    #[test]
    fn test_pipeline_iii_parsing() {
        let json = pipeline_iii();
        let graph = PolicyGraph::from(&json);
        println!("{}", graph);
        assert_eq!(graph.nodes.len(), 5);
        assert_eq!(graph.edges.len(), 4);
        assert_eq!(graph.n_devices, 2);
        assert_eq!(graph.network_nodes.len(), 1);
        assert_eq!(graph.pipelines.len(), 2);
    }

    fn figure_3ab() -> GraphJson {
        GraphJson {
            sensors: vec![
                sensor_json("speaker", vec!["speech_command"], vec!["playback"]),
                sensor_json("speaker_1", vec!["speech_command"], vec!["playback"]),
                sensor_json("light", vec!["state", "intensity"], vec!["state", "intensity"]),
            ],
            moduleIds: vec![
                module_json("picovoice", vec!["speech"], vec!["weather_intent", "light_intent"]),
                module_json("weather", vec!["weather_intent"], vec!["weather"]),
                module_json("light_switch", vec!["light_intent"], vec!["state"]),
                module_json("set_true", vec![], vec!["true"]),
                module_json("set_false", vec![], vec!["false"]),
            ],
            dataEdges: vec![(true, 0, 0, 3, 0), (false, 1, 0, 3, 0), (true, 3, 0, 4, 0), (true, 3, 1, 5, 0)],
            stateEdges: vec![(4, 0, 0, 0), (5, 0, 2, 0), (6, 0, 2, 0), (7, 0, 2, 0)],
            networkEdges: vec![(4, "weather.com".to_string())],
            intervals: vec![],
        }
    }

    #[test]
    fn test_figure_3ab_parsing() {
        let json = figure_3ab();
        let graph = PolicyGraph::from(&json);
        assert_eq!(graph.nodes.len(), 8);
        assert_eq!(graph.edges.len(), 8);
        assert_eq!(graph.n_devices, 3);
        assert_eq!(graph.network_nodes.len(), 1);
        assert_eq!(graph.pipelines.len(), 6);
    }

    #[test]
    fn test_figure_3ab_pipeline_order() {
        let graph = PolicyGraph::from(&figure_3ab());
        let perms = &graph.pipelines;
        assert_eq!(perms.len(), 6);
        assert_eq!(perms[0].get_source(), &PipelineNode::Network { domain: "weather.com".to_string() });
        assert_eq!(perms[1].get_source(), &PipelineNode::Data { device: 0, output: 0 });
        assert_eq!(perms[2].get_source(), &PipelineNode::Data { device: 1, output: 0 });
        assert_eq!(perms[3].get_source(), &PipelineNode::Data { device: 0, output: 0 });
        assert_eq!(perms[4].get_source(), &PipelineNode::Data { device: 1, output: 0 });
        assert_eq!(perms[5].get_source(), &PipelineNode::Data { device: 1, output: 0 });
        assert_eq!(perms[0].len(), 3);
        assert_eq!(perms[1].len(), 5);
        assert_eq!(perms[2].len(), 5);
        assert_eq!(perms[3].len(), 6);
        assert_eq!(perms[4].len(), 6);
        assert_eq!(perms[5].len(), 6);
        // assert_eq!(perms[0].get_sink(), &PipelineNode::Data { device: 0, output: 0 });
        // assert_eq!(perms[1].get_sink(), &PipelineNode::Data { device: 0, output: 0 });
        // assert_eq!(perms[2].get_sink(), &PipelineNode::Data { device: 0, output: 0 });
        // assert_eq!(perms[3].get_sink(), &PipelineNode::Data { device: 0, output: 0 });
        // assert_eq!(perms[4].get_sink(), &PipelineNode::Data { device: 0, output: 0 });
    }

    #[test]
    fn test_remove_one_light_pipeline() {

    }

    #[test]
    fn test_remove_cross_speaker_pipelines() {

    }


}
