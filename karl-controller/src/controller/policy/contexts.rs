use super::graph::{Pipeline, PipelineNode};

#[derive(Debug, Clone)]
pub enum SecurityContext {
    TruePrivate,
    FalsePublic,
    Module(usize, String),
}

impl SecurityContext {
    pub fn conflicts_with(&self, pipeline: &Pipeline) -> bool {
        let module_to_match = match self {
            SecurityContext::TruePrivate => { return true; },
            SecurityContext::FalsePublic => { return false; },
            SecurityContext::Module(module_i, _) => module_i,
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
            SecurityContext::Module(_, module) => module.clone(),
        }
    }
}
