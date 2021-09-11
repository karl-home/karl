mod graph;
pub use graph::PolicyGraph;

pub struct KarlPolicy {
    base_graph: PolicyGraph,
    pipeline: Vec<graph::Pipeline>,
    contexts: Vec<graph::SecurityContext>,
}

impl KarlPolicy {

}
