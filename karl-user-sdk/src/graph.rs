use std::fmt::Write;
use std::collections::HashMap;

type SensorKey = (String, String);
type ModuleParam = (String, String);
/// All module returns are also entity returns
type ModuleReturn = (String, String);
type EntityReturn = (String, String);
type Sensor = String;
type Module = String;
type Domain = String;

#[derive(Debug, Default, Clone)]
pub struct Graph {
    pub sensors: Vec<Sensor>,
    pub modules: Vec<Module>,
    pub data_edges: Vec<(EntityReturn, ModuleParam, bool)>,
    pub state_edges: Vec<(ModuleReturn, SensorKey)>,
    pub network_edges: Vec<(Module, Domain)>,
    pub interval_modules: Vec<(Module, u32)>,
}

impl Graph {
    pub fn new(
        sensors: Vec<&str>,
        modules: Vec<&str>,
        data_edges_stateless: Vec<(&str, &str)>,
        data_edges_stateful: Vec<(&str, &str)>,
        state_edges: Vec<(&str, &str)>,
        network_edges: Vec<(&str, &str)>,
        interval_modules: Vec<(&str, u32)>,
    ) -> Self {
        let mut data_edges = data_edges_stateless.into_iter()
            .map(|(a, b)| (split(a), split(b), true)).collect();
        let mut data_edges_stateful = data_edges_stateful.into_iter()
            .map(|(a, b)| (split(a), split(b), false)).collect();
        data_edges_stateless.append(&mut data_edges_stateful);
        Graph {
            sensors: sensors.into_iter().map(|a| a.to_string()).collect(),
            modules: modules.into_iter().map(|a| a.to_string()).collect(),
            data_edges,
            state_edges: state_edges.into_iter()
                .map(|(a, b)| (split(a), split(b))).collect(),
            network_edges: network_edges.into_iter()
                .map(|(a, b)| (a.to_string(), b.to_string())).collect(),
            interval_modules: interval_modules.into_iter()
                .map(|(a, b)| (a.to_string(), b)).collect(),
        }
    }

    /// Takes a map from global module ID to registered module ID.
    pub fn replace_module_ids(
        &mut self,
        global_map: &HashMap<Module, Module>,
    ) {
        let mut modules: Vec<&mut Module> = vec![];
        for m in self.modules.iter_mut() {
            modules.push(m);
        }
        for ((m1, _), (m2, _), _) in self.data_edges.iter_mut() {
            modules.push(m1);
            modules.push(m2);
        }
        for ((m1, _), (m2, _)) in self.state_edges.iter_mut() {
            modules.push(m1);
            modules.push(m2);
        }
        for (m, _) in self.network_edges.iter_mut() {
            modules.push(m);
        }
        for (m, _) in self.interval_modules.iter_mut() {
            modules.push(m);
        }
        for m in modules {
            if let Some(m_new) = global_map.get(m) {
                *m = m_new.to_string();
            }
        }
    }

    pub fn graphviz(&self) -> Result<String, std::fmt::Error> {
        let mut g = String::new();
        writeln!(g, "digraph G {{")?;
        // sensors
        writeln!(g, "  subgraph cluster_0 {{")?;
        writeln!(g, "    rank=\"source\";")?;
        writeln!(g, "    label=\"Sensors\";")?;
        writeln!(g, "    node [shape=hexagon];")?;
        for sensor in &self.sensors {
            writeln!(g, "    {};", sensor)?;
        }
        writeln!(g, "  }}")?;
        // modules
        writeln!(g, "\n  node [shape=box];")?;
        for module in &self.modules {
            writeln!(g, "  {};", module)?;
        }
        // stateless data edges
        writeln!(g, "\n  edge [style=solid];")?;
        for ((m1, t1), (m2, t2), stateless) in &self.data_edges_stateless {
            if stateless {
                writeln!(g, "  {} -> {} [label=\"{},{}\"];", m1, m2, t1, t2)?;
            }
        }
        // stateful data edges
        writeln!(g, "\n  edge [style=dashed];")?;
        for ((m1, t1), (m2, t2), stateless) in &self.data_edges_stateful {
            if !stateless {
                writeln!(g, "  {} -> {} [label=\"{},{}\"];", m1, m2, t1, t2)?;
            }
        }
        // state edges
        writeln!(g, "\n  edge [style=solid,color=orange];")?;
        for ((m1, tag), (m2, key)) in &self.state_edges {
            writeln!(g, "  {} -> {} [label=\"{},{}\"];", m1, m2, tag, key)?;
        }
        // network edges
        writeln!(g, "\n  edge [style=solid,color=green];")?;
        for (module, domain) in &self.network_edges {
            writeln!(g, "  {} -> net [label=\"{}\"];", module, domain)?;
        }
        // interval modules
        writeln!(g, "")?;
        for (module, interval) in &self.interval_modules {
            writeln!(g, "  {} [peripheries=2,label=\"{} ({}s)\"];",
                module, module, interval)?;
        }
        // fin.
        writeln!(g, "  net [shape=ribosite];")?;
        writeln!(g, "}}")?;
        Ok(g)
    }

    pub async fn send_to_controller(
        &self,
        api: &crate::net::KarlUserSDK,
    ) -> Result<(), tonic::Status> {
        let data_edges = self.data_edges.clone().into_iter()
            .map(|((out_id, out_return), (in_id, in_param), stateless)| {
                DataEdge { out_id, out_return, in_id, in_param, stateless }})
            .collect();
        let state_edges = self.state_edges.clone().into_iter()
            .map(|((out_id, out_return), (sensor_id, sensor_key))| {
                StateEdge { out_id, out_return, sensor_id, sensor_key }})
            .collect();
        let network_edges = self.network_edges.clone().into_iter()
            .map(|(module_id, domain)| {
                NetworkEdge { module_id, domain }})
            .collect();
        let intervals = self.intervals.clone().into_iter()
            .map(|(module_id, seconds)| {
                Interval { module_id, seconds }})
            .collect();
        let req = GraphRequest {
            data_edges,
            state_edges,
            network_edges,
            intervals,
        };
        api.set_graph(req)
    }
}

fn split(tag: &str) -> (String, String) {
    let mut split = tag.split(".");
    (split.next().unwrap().to_string(), split.next().unwrap().to_string())
}
