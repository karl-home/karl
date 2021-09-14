use std::collections::HashMap;
use karl_common::*;

mod contexts;
pub(crate) mod graph;
use contexts::SecurityContext;
use graph::{PipelineNode, PolicyGraph};
use crate::{GraphJson, PolicyJson};

#[derive(Debug, Clone)]
struct PipelineInner {
    allowed: bool,
    conflicts: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PrivacyPolicies {
    pub json: GraphJson,
    base_graph: PolicyGraph,
    real_graph: PolicyGraph,
    /// The number of pipelines must be the same as the number of
    /// pipelines in the base_graph.
    pipelines: Vec<PipelineInner>,
    contexts: HashMap<PipelineNode, SecurityContext>,
}

impl PrivacyPolicies {
    pub fn add_sensor(
        &mut self,
        id: String,
        inputs: Vec<String>,
        outputs: Vec<String>,
    ) {
        if !self.json.moduleIds.is_empty() {
            unimplemented!("currently can only register devices if no modules are registered");
        }
        self.json.sensors.push(crate::SensorJson {
            id,
            stateKeys: inputs.into_iter().map(|input| (input, String::from("-"))).collect(),
            returns: outputs.into_iter().map(|output| (output, String::from("-"))).collect(),
        });
        self.base_graph = PolicyGraph::from(&self.json);
        self.real_graph = self.base_graph.clone();
    }

    pub fn save_graph(&mut self, json: GraphJson) {
        self.json = json;
        self.base_graph = PolicyGraph::from(&self.json);
        // TODO: When changing the graph, we may want to preserve
        // any previously denied pipelines since the user might
        // not want to automatically approve all of them. Or vice
        // versa by preserving allowed pipelines only.
        self.pipelines = (0..self.base_graph.pipelines.len())
            .map(|_| PipelineInner {
                allowed: true,
                conflicts: false,
            }).collect();
        // Update conflicting pipelines based on security contexts
        self.mark_conflicting_policies();
        let removed = self.pipelines
            .iter()
            .enumerate()
            .filter(|(_, inner)| !inner.allowed || inner.conflicts)
            .map(|(i, _)| i)
            .collect();
        self.real_graph = self.base_graph.apply_removed_pipelines(removed);
    }

    fn mark_conflicting_policies(&mut self) {
        // For each pipeline that is currently allowed and each context...
        debug!("marking conflicting policies");
        assert_eq!(self.pipelines.len(), self.base_graph.pipelines.len());
        for (i, inner) in self.pipelines.iter_mut().enumerate() {
            if !inner.allowed {
                continue;
            }
            for (pnode, ctx) in self.contexts.iter() {
                // ...if the pipeline contains the pipeline node for which
                // the context is defined, and the pipeline conflicts with
                // the context, mark the pipeline as disallowed.
                let pipeline = &self.base_graph.pipelines[i];
                if pipeline.contains_node(pnode) {
                    if ctx.conflicts_with(pipeline, &self.base_graph.node_map) {
                        debug!("context {:?} conflicts with pipeline {}", ctx, i);
                        inner.conflicts = true;
                        break;
                    }
                }
            }
        }
    }

    pub fn save_policies(&mut self, json: PolicyJson) -> Result<(), String> {
        // Assumes the graph hasn't changed since user updated pipeline
        // policies, since they are updated by index.
        if self.pipelines.len() != json.pipelines.len() {
            return Err(format!(
                "policies apply to outdated graph expected {} != actual {}",
                self.pipelines.len(), json.pipelines.len(),
            ));
        }
        // Mark any changes in whether pipelines are allowed.
        for (i, (_, allowed)) in json.pipelines.into_iter().enumerate() {
            if self.pipelines[i].allowed != allowed {
                info!("pipeline {} allowed? {} -> {}", i,
                    self.pipelines[i].allowed, allowed);
                self.pipelines[i].allowed = allowed;
            }
        }
        // Parse the security contexts
        self.contexts.clear();
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
            let pnode = if node < self.base_graph.n_devices {
                if is_input {
                    PipelineNode::Actuator { device: node, input: index }
                } else {
                    PipelineNode::Data { device: node, output: index }
                }
            } else {
                if is_input {
                    PipelineNode::ModuleInput { module: node, index }
                } else {
                    PipelineNode::ModuleOutput { module: node, index }
                }
            };
            self.contexts.insert(pnode, ctx);
        }
        // Mark any pipeline policies as `conflicts` if they conflict
        // with any security contexts.
        self.mark_conflicting_policies();
        let removed = self.pipelines
            .iter()
            .enumerate()
            .filter(|(_, inner)| !inner.allowed || inner.conflicts)
            .map(|(i, _)| i)
            .collect();
        self.real_graph = self.base_graph.apply_removed_pipelines(removed);
        Ok(())
    }

    pub fn get_pipeline_strings(&self) -> Vec<(String, bool)> {
        self.pipelines
            .iter()
            .enumerate()
            .map(|(i, inner)| {
                let pipeline = self.base_graph.pipelines[i].clone();
                let mut nodes = vec![pipeline.source];
                for node in pipeline.nodes {
                    let last_node = &nodes[nodes.len() - 1];
                    if last_node.is_module_input() && node.is_module_output() {
                        continue;
                    }
                    nodes.push(node);
                }
                let nodes = nodes.into_iter()
                    .map(|node| self.base_graph.pnode_to_string(node))
                    .collect::<Vec<_>>();
                (nodes.join(" -> "), inner.allowed)
            })
            .collect()
    }

    pub fn get_security_context_strings(&self) -> Vec<(String, String)> {
        let mut contexts = Vec::new();
        for (node, context) in self.contexts.iter() {
            let tag = match node {
                PipelineNode::Data { device, output } => {
                    let node = &self.base_graph.nodes[*device];
                    format!("{}.{}", node.id, node.outputs[*output])
                },
                PipelineNode::ModuleOutput { module, index: output } => {
                    let node = &self.base_graph.nodes[*module];
                    format!("{}.{}", node.id, node.outputs[*output])
                },
                PipelineNode::ModuleInput { module, index: input } => {
                    let node = &self.base_graph.nodes[*module];
                    format!("{}.{}", node.id, node.inputs[*input])
                },
                PipelineNode::Actuator { device, input } => {
                    let node = &self.base_graph.nodes[*device];
                    format!("#{}.{}", node.id, node.inputs[*input])
                },
                PipelineNode::Network { .. } => unimplemented!(),
            };
            contexts.push((tag, context.to_string()));
        }
        info!("security contexts string: {:?}", contexts);
        contexts
    }
}
