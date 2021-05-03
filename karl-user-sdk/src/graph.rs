use std::fmt::Write;
use std::collections::HashMap;

/// All SensorKeys are also ModuleTags
type SensorKey = (String, String);
type ModuleTag = (String, String);
type Sensor = String;
type Module = String;
type Domain = String;

#[derive(Debug, Default, Clone)]
pub struct Graph {
    pub sensors: Vec<Sensor>,
    pub modules: Vec<Module>,
    pub data_edges_stateless: Vec<(ModuleTag, Module)>,
    pub data_edges_stateful: Vec<(ModuleTag, Module)>,
    pub state_edges: Vec<(ModuleTag, SensorKey)>,
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
        Graph {
            sensors: sensors.into_iter().map(|a| a.to_string()).collect(),
            modules: modules.into_iter().map(|a| a.to_string()).collect(),
            data_edges_stateless: data_edges_stateless.into_iter()
                .map(|(a, b)| (split(a), b.to_string())).collect(),
            data_edges_stateful: data_edges_stateful.into_iter()
                .map(|(a, b)| (split(a), b.to_string())).collect(),
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
        for ((m1, _), m2) in self.data_edges_stateless.iter_mut() {
            modules.push(m1);
            modules.push(m2);
        }
        for ((m1, _), m2) in self.data_edges_stateful.iter_mut() {
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
        for ((m1, tag), m2) in &self.data_edges_stateless {
            writeln!(g, "  {} -> {} [label=\"{}\"];", m1, m2, tag)?;
        }
        // stateful data edges
        writeln!(g, "\n  edge [style=dashed];")?;
        for ((m1, tag), m2) in &self.data_edges_stateful {
            writeln!(g, "  {} -> {} [label=\"{}\"];", m1, m2, tag)?;
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
        for ((m1, tag), m2) in self.data_edges_stateless.clone() {
            api.add_data_edge(m1, tag, m2, true).await?;
        }
        for ((m1, tag), m2) in self.data_edges_stateful.clone() {
            api.add_data_edge(m1, tag, m2, false).await?;
        }
        for ((m, tag), (s, key)) in self.state_edges.clone() {
            api.add_state_edge(m, tag, s, key).await?;
        }
        for (m, domain) in self.network_edges.clone() {
            api.add_network_edge(m, domain).await?;
        }
        for (m, interval) in self.interval_modules.clone() {
            api.set_interval(m, interval).await?;
        }
        Ok(())
    }
}

fn split(tag: &str) -> ModuleTag {
    let mut split = tag.split(".");
    (split.next().unwrap().to_string(), split.next().unwrap().to_string())
}
