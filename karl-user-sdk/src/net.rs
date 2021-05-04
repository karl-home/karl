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
        input_param: String,
        stateless: bool,
    ) -> Result<(), Status> {
        let request = DataEdgeRequest {
            output_id,
            output_tag,
            input_id,
            input_param,
            stateless,
            add: true,
        };
        KarlControllerClient::connect(self.controller_addr.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .data_edge(Request::new(request)).await
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
        let request = StateEdgeRequest {
            output_id,
            output_tag,
            input_id,
            input_key,
            add: true,
        };
        KarlControllerClient::connect(self.controller_addr.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .state_edge(Request::new(request)).await
            .map(|res| res.into_inner())
    }

    /// Adds network edge.
    pub async fn add_network_edge(
        &self,
        output_id: String,
        domain: String,
    ) -> Result<(), Status> {
        let request = NetworkEdgeRequest {
            output_id,
            domain,
            add: true,
        };
        KarlControllerClient::connect(self.controller_addr.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .network_edge(Request::new(request)).await
            .map(|res| res.into_inner())
    }

    /// Sets interval schedule.
    pub async fn set_interval(
        &self,
        hook_id: String,
        seconds: u32,
    ) -> Result<(), Status> {
        let request = IntervalRequest {
            hook_id,
            seconds,
            add: true,
        };
        KarlControllerClient::connect(self.controller_addr.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .interval(Request::new(request)).await
            .map(|res| res.into_inner())
    }

    /// Removes data edge.
    pub async fn remove_data_edge(
        &self,
        output_id: String,
        output_tag: String,
        input_id: String,
        input_param: String,
        stateless: bool,
    ) -> Result<(), Status> {
        let request = DataEdgeRequest {
            output_id,
            output_tag,
            input_id,
            input_param,
            stateless,
            add: false,
        };
        KarlControllerClient::connect(self.controller_addr.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .data_edge(Request::new(request)).await
            .map(|res| res.into_inner())
    }

    /// Removes state edge.
    pub async fn remove_state_edge(
        &self,
        output_id: String,
        output_tag: String,
        input_id: String,
        input_key: String,
    ) -> Result<(), Status> {
        let request = StateEdgeRequest {
            output_id,
            output_tag,
            input_id,
            input_key,
            add: false,
        };
        KarlControllerClient::connect(self.controller_addr.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .state_edge(Request::new(request)).await
            .map(|res| res.into_inner())
    }

    /// Removes network edge.
    pub async fn remove_network_edge(
        &self,
        output_id: String,
        domain: String,
    ) -> Result<(), Status> {
        let request = NetworkEdgeRequest {
            output_id,
            domain,
            add: false,
        };
        KarlControllerClient::connect(self.controller_addr.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .network_edge(Request::new(request)).await
            .map(|res| res.into_inner())
    }

    /// Sets interval schedule.
    pub async fn remove_interval(
        &self,
        hook_id: String,
        seconds: u32,
    ) -> Result<(), Status> {
        let request = IntervalRequest {
            hook_id,
            seconds,
            add: false,
        };
        KarlControllerClient::connect(self.controller_addr.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .interval(Request::new(request)).await
            .map(|res| res.into_inner())
    }
}
