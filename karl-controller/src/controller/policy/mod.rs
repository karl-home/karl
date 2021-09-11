use std::collections::HashMap;
use karl_common::*;

mod contexts;
pub(crate) mod graph;
use contexts::SecurityContext;
use graph::{EdgeNode, PolicyGraph};

#[derive(Debug, Clone, Default)]
pub struct PrivacyPolicies {
    base_graph: PolicyGraph,
    real_graph: PolicyGraph,
    input_contexts: HashMap<graph::EdgeNode, SecurityContext>,
    output_contexts: HashMap<graph::EdgeNode, SecurityContext>,
}

impl PrivacyPolicies {
    pub fn save_graph(&mut self, json: &crate::GraphJson) -> Result<(), String> {
        self.base_graph = PolicyGraph::from(json);
        self.real_graph = self.base_graph.clone();
        self.input_contexts.clear();
        self.output_contexts.clear();
        for (tag, ctx) in &json.contexts {
            info!("set context tag={} ctx={}", tag, ctx);
            if !tag_parsing::is_tag(&tag) {
                return Err(format!("security context for malformed tag={}", tag));
            }
            let &(node, index, is_input) = self.base_graph.tag_map.get(tag)
                .ok_or(format!("tag {} does not exist", tag))?;
            let ctx = if ctx == "PRIVATE" {
                SecurityContext::TruePrivate
            } else if ctx == "PUBLIC" {
                SecurityContext::FalsePublic
            } else {
                SecurityContext::Module(ctx.to_string())
            };
            if is_input {
                self.input_contexts.insert(EdgeNode { node, index }, ctx);
            } else {
                self.output_contexts.insert(EdgeNode { node, index }, ctx);
            }
        }
        Ok(())
    }

    pub fn get_pipeline_strings(&self) -> Vec<(String, bool)> {
        self.base_graph
            .get_pipelines()
            .into_iter()
            .map(|pipeline| {
                let mut nodes = pipeline.nodes.into_iter()
                    .map(|node| self.base_graph.pnode_to_string(node))
                    .collect::<Vec<_>>();
                nodes.insert(0, self.base_graph.pnode_to_string(pipeline.source));
                nodes.join(" -> ")
            })
            .map(|string| (string, true))
            .collect()
    }

    pub fn get_security_context_strings(&self) -> Vec<(String, String)> {
        let mut contexts = Vec::new();
        for (edge_node, context) in self.input_contexts.iter() {
            let (node_i, input_i) = (edge_node.node, edge_node.index);
            let node = &self.base_graph.nodes[node_i];
            let tag = if node_i < self.base_graph.n_devices {
                format!("#{}.{}", node.id, node.inputs[input_i])
            } else {
                format!("{}.{}", node.id, node.inputs[input_i])
            };
            contexts.push((tag, context.to_string()));
        }
        for (edge_node, context) in self.output_contexts.iter() {
            let (node_i, output_i) = (edge_node.node, edge_node.index);
            let node = &self.base_graph.nodes[node_i];
            let tag = format!("{}.{}", node.id, node.outputs[output_i]);
            contexts.push((tag, context.to_string()));
        }
        info!("security contexts string: {:?}", contexts);
        contexts
    }
}
