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
use crate::protos2::karl_controller_client::KarlControllerClient;
use crate::protos2::karl_host_client::KarlHostClient;
use crate::protos2::*;
use crate::common::*;

/// Registers a hook.
pub async fn register_hook(
    controller_addr: &str,
    client_token: &str,
    global_hook_id: &str,
) -> Result<Response<()>, Status> {
    let mut client = KarlControllerClient::connect(controller_addr.to_string())
        .await.map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?;
    let request = Request::new(RegisterHookRequest {
        user_token: "TEMPORARY".to_string(),
        global_hook_id: global_hook_id.to_string(),
        envs: vec![],
        file_perm: vec![],
        network_perm: vec![],
        client_token: client_token.to_string(),
    });
    client.register_hook(request).await
}

/// Sends a compute request to the given host and returns the result.
pub async fn send_compute(
    host: &str,
    req: ComputeRequest,
) -> Result<Response<()>, Status> {
    let mut client = KarlHostClient::connect(host.to_string())
        .await.map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?;
    let request = Request::new(req);
    client.start_compute(request).await
}

/*****************************************************************************
 * Service API
 *****************************************************************************/
/// Notify controller about compute request start.
pub async fn notify_start(
    controller_addr: &str, service_id: u32, description: String,
) -> Result<Response<()>, Status> {
    let mut client = KarlControllerClient::connect(controller_addr.to_string())
        .await.map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?;
    let service_name = format!("KarlService-{}", service_id);
    let request = Request::new(NotifyStart {
        process_token: "TEMPORARY".to_string(),
        service_name,
        description,
    });
    debug!("notify start");
    client.start_compute(request).await
}

/// Notify controller about compute request end.
pub async fn notify_end(
    controller_addr: &str, service_id: u32, token: RequestToken,
) -> Result<Response<()>, Status> {
    let mut client = KarlControllerClient::connect(controller_addr.to_string())
        .await.map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?;
    let service_name = format!("KarlService-{}", service_id);
    let request = Request::new(NotifyEnd {
        host_token: "TEMPORARY".to_string(),
        process_token: "TEMPORARY".to_string(),
        service_name,
        request_token: token.0,
    });
    debug!("notify_end");
    client.finish_compute(request).await
}

/// Send a heartbeat to the controller.
pub async fn heartbeat(
    controller_addr: &str, service_id: u32, token: Option<RequestToken>,
) -> Result<Response<()>, Status> {
    let mut client = KarlControllerClient::connect(controller_addr.to_string())
        .await.map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?;
    let service_name = format!("KarlService-{}", service_id);
    let request = Request::new(HostHeartbeat {
        host_token: "TEMPORARY".to_string(),
        service_name,
        request_token: token.unwrap_or(Token("".to_string())).0,
    });
    client.heartbeat(request).await
}

/// Register a host with the controller.
pub async fn register_host(
    controller_addr: &str, service_id: u32, port: u16, password: &str,
) -> Result<Response<HostRegisterResult>, Status> {
    let mut client = KarlControllerClient::connect(controller_addr.to_string())
        .await.map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?;
    let service_name = format!("KarlService-{}", service_id);
    let ip = std::net::UdpSocket::bind("0.0.0.0:0").unwrap()
        .local_addr().unwrap().ip().to_string();
    let request = Request::new(HostRegisterRequest {
        host_id: service_name.clone(),
        ip,
        port: port as _,
        password: password.to_string(),
        service_name: service_name,
    });
    client.host_register(request).await
}
