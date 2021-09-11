use std::collections::HashMap;
use karl_common::*;

mod graph;
use graph::{EdgeNode, Pipeline, PipelineNode};
pub use graph::PolicyGraph;

pub enum SecurityContext {
    TruePrivate,
    FalsePublic,
    Module(usize),
}

pub struct KarlPolicy {
    base_graph: PolicyGraph,
    real_graph: PolicyGraph,
    input_contexts: HashMap<graph::EdgeNode, SecurityContext>,
    output_contexts: HashMap<graph::EdgeNode, SecurityContext>,
}

impl SecurityContext {
    fn conflicts_with(&self, pipeline: &Pipeline) -> bool {
        let module_to_match = match self {
            SecurityContext::TruePrivate => { return true; },
            SecurityContext::FalsePublic => { return false; },
            SecurityContext::Module(module) => module,
        };
        for node in &pipeline.nodes {
            match node {
                PipelineNode::ModuleOutput { module, .. } => {
                    if module == module_to_match {
                        return true;
                    }
                }
                _ => {},
            }
        }
        false
    }
}

impl KarlPolicy {
    pub fn new(json: &crate::GraphJson) -> Self {
        let graph = PolicyGraph::from(json);
        Self {
            base_graph: graph.clone(),
            real_graph: graph,
            input_contexts: HashMap::new(),
            output_contexts: HashMap::new(),
        }
    }

    pub fn add_security_context(
        &mut self,
        tag: karl_common::Tag,
        context: SecurityContext,
    ) {
        assert!(tag_parsing::is_tag(&tag));
        let &(node, index, is_input) = self.base_graph.node_map.get(&tag).unwrap();
        if is_input {
            self.input_contexts.insert(EdgeNode { node, index }, context);
        } else {
            self.output_contexts.insert(EdgeNode { node, index }, context);
        }
    }
}
