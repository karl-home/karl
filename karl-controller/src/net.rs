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
use tonic::{Request, Response, Status, Code};
use crate::protos::karl_controller_client::KarlControllerClient;
use crate::protos::karl_host_client::KarlHostClient;
use crate::protos::*;

/// Sends a compute request to the given host and returns the result.
/// The only method in the Controller API.
pub async fn send_compute(
    host: &str,
    req: ComputeRequest,
) -> Result<Response<NotifyStart>, Status> {
    KarlHostClient::connect(host.to_string()).await
        .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
        .start_compute(Request::new(req)).await
}
