use std::collections::HashMap;
use karl_common::*;

mod contexts;
pub(crate) mod graph;
use contexts::SecurityContext;
use graph::{Pipeline, PipelineNode, PolicyGraph};
use crate::{GraphJson, PolicyJson};

#[derive(Debug, Clone)]
struct PipelineInner {
    pipeline: Pipeline,
    allowed: bool,
    conflicts: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PrivacyPolicies {
    pub json: GraphJson,
    base_graph: PolicyGraph,
    real_graph: PolicyGraph,
    /// The boolean states whether the pipeline is allowed.
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
        self.real_graph = self.base_graph.clone();
        self.pipelines = self.base_graph.get_pipelines().into_iter()
            .map(|pipeline| PipelineInner {
                pipeline: pipeline,
                allowed: true,
                conflicts: false,
            }).collect();
        self.mark_conflicting_policies();
    }

    fn mark_conflicting_policies(&mut self) {
        // For each pipeline that is currently allowed and each context...
        debug!("marking conflicting policies");
        for (i, inner) in self.pipelines.iter_mut().enumerate() {
            if !inner.allowed {
                continue;
            }
            for (pnode, ctx) in self.contexts.iter() {
                // ...if the pipeline contains the pipeline node for which
                // the context is defined, and the pipeline conflicts with
                // the context, mark the pipeline as disallowed.
                let pipeline = &inner.pipeline;
                if pipeline.contains_node(pnode) {
                    if ctx.conflicts_with(pipeline, &self.base_graph.node_map) {
                        debug!("context {:?} conflicts with pipeline {}", ctx, i);
                        inner.conflicts = true;
                        break;
                    }
                }
            }
        }
        for (i, inner) in self.pipelines.iter().enumerate() {
            debug!("remove pipeline {}", i);
            if !inner.allowed || inner.conflicts {
                let p = &inner.pipeline;
                // The index must be in bounds because a pipeline policy must
                // have 2 nodes, and only a module can have network access.
                let node_before_sink = &p.nodes[p.nodes.len() - 2];
                match p.get_sink() {
                    PipelineNode::Network { domain } => {
                        let module_i = match node_before_sink {
                            PipelineNode::ModuleInput { module, .. } => module,
                            _ => unreachable!(),
                        };
                        let module_id = &self.base_graph.nodes[*module_i].id;
                        info!("revoke network access to {} from {}", domain, module_id);
                        // TODO
                    }
                    PipelineNode::Actuator { device, input } => {
                        let (module_i, output_i) = match node_before_sink {
                            PipelineNode::ModuleOutput { module, index } =>
                                (module, index),
                            _ => unreachable!(),
                        };
                        let module = &self.base_graph.nodes[*module_i];
                        let device = &self.base_graph.nodes[*device];
                        let output = &module.outputs[*output_i];
                        let input = &device.inputs[*input];
                        info!("remove edge from {}.{} to #{}.{}",
                            module.id, output, device.id, input);
                        // TODO
                    }
                    _ => unreachable!(),
                }
            }
        }
    }

    pub fn save_policies(&mut self, json: PolicyJson) -> Result<(), String> {
        if self.pipelines.len() != json.pipelines.len() {
            return Err(format!(
                "policies apply to outdated graph expected {} != actual {}",
                self.pipelines.len(), json.pipelines.len(),
            ));
        }
        for (i, (_, allowed)) in json.pipelines.into_iter().enumerate() {
            if self.pipelines[i].allowed != allowed {
                info!("pipeline {} allowed? {} -> {}", i,
                    self.pipelines[i].allowed, allowed);
                self.pipelines[i].allowed = allowed;
            }
        }
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
        self.mark_conflicting_policies();
        Ok(())
    }

    pub fn get_pipeline_strings(&self) -> Vec<(String, bool)> {
        self.pipelines
            .clone()
            .into_iter()
            .map(|inner| {
                let pipeline = inner.pipeline;
                let mut nodes = pipeline.nodes.into_iter()
                    .map(|node| self.base_graph.pnode_to_string(node))
                    .collect::<Vec<_>>();
                nodes.insert(0, self.base_graph.pnode_to_string(pipeline.source));
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
