//! The client for interacting with the Karl controller and services.
//!
//! Each language SDK must re-implement its own client, called an executor.
//! The interactions with other Karl entities are entirely through TCP.
//! The interface must follow the request and result interface defined in
//! `common.rs`.
//!
//! If the request to a host is unsuccessful, the client must query the
//! executor for a different host and try again on the client-side.
//! Addresses are passed in the form of `<IP>:<PORT>`.
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

    /// Adds data edge.
    pub async fn add_data_edge(
        &self,
        output_id: String,
        output_tag: String,
        input_id: String,
        stateless: bool,
    ) -> Result<(), Status> {
        let request = AddDataEdgeRequest {
            output_id,
            output_tag,
            input_id,
            stateless,
        };
        KarlControllerClient::connect(self.controller_addr.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .add_data_edge(Request::new(request)).await
            .map(|res| res.into_inner())
    }

    /// Adds state edge.
    pub async fn add_state_edge(
        &self,
        output_id: String,
        output_tag: String,
        input_id: String,
        input_key: String,
    ) -> Result<(), Status> {
        let request = AddStateEdgeRequest {
            output_id,
            output_tag,
            input_id,
            input_key,
        };
        KarlControllerClient::connect(self.controller_addr.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .add_state_edge(Request::new(request)).await
            .map(|res| res.into_inner())
    }

    /// Adds network edge.
    pub async fn add_network_edge(
        &self,
        output_id: String,
        domain: String,
    ) -> Result<(), Status> {
        let request = AddNetworkEdgeRequest {
            output_id,
            domain,
        };
        KarlControllerClient::connect(self.controller_addr.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .add_network_edge(Request::new(request)).await
            .map(|res| res.into_inner())
    }

    /// Adds network edge.
    pub async fn set_interval(
        &self,
        hook_id: String,
        seconds: u32,
    ) -> Result<(), Status> {
        let request = SetIntervalRequest {
            hook_id,
            seconds,
        };
        KarlControllerClient::connect(self.controller_addr.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .set_interval(Request::new(request)).await
            .map(|res| res.into_inner())
    }
}
