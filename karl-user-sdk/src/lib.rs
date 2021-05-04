pub mod protos {
    tonic::include_proto!("request");
}

mod graph;
pub use graph::Graph;

use tonic::{Request, Status, Code};
use crate::protos::karl_controller_client::KarlControllerClient;
use crate::protos::*;

#[derive(Debug, Clone)]
pub struct KarlUserSDK {
    pub controller_addr: String,
}

impl KarlUserSDK {
    pub fn new(controller_addr: &str) -> Self {
        Self {
            controller_addr: controller_addr.to_string(),
        }
    }

    /// Registers a hook.
    pub async fn register_hook(
        &self,
        global_hook_id: &str,
    ) -> Result<RegisterHookResult, Status> {
        let request = RegisterHookRequest {
            token: "".to_string(),
            global_hook_id: global_hook_id.to_string(),
        };
        KarlControllerClient::connect(self.controller_addr.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .register_hook(Request::new(request)).await
            .map(|res| res.into_inner())
    }

    pub async fn set_graph(
        &self,
        request: GraphRequest,
    ) -> Result<(), Status> {
        KarlControllerClient::connect(self.controller_addr.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .set_graph(Request::new(request)).await
            .map(|res| res.into_inner())
    }
}
