use std::collections::HashMap;
use super::graph::{Pipeline, PipelineNode};

#[derive(Debug, Clone)]
pub enum SecurityContext {
    TruePrivate,
    FalsePublic,
    Module(String),
}

impl SecurityContext {
    pub fn conflicts_with(
        &self,
        pipeline: &Pipeline,
        node_map: &HashMap<String, usize>,
    ) -> bool {
        let module_to_match = match self {
            SecurityContext::TruePrivate => { return true; },
            SecurityContext::FalsePublic => { return false; },
            SecurityContext::Module(module) => {
                if let Some(module_i) = node_map.get(module) {
                    module_i
                } else {
                    return false;
                }
            },
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

    pub fn to_string(&self) -> String {
        match self {
            SecurityContext::TruePrivate => String::from("PRIVATE"),
            SecurityContext::FalsePublic => String::from("PUBLIC"),
            SecurityContext::Module(module) => module.clone(),
        }
    }
}
