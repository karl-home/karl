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
use crate::common::*;

/// Registers a hook.
pub async fn register_hook(
    controller_addr: &str,
    sensor_token: &str,
    global_hook_id: &str,
) -> Result<Response<()>, Status> {
    let mut client = KarlControllerClient::connect(controller_addr.to_string())
        .await.map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?;
    let request = RegisterHookRequest {
        token: sensor_token.to_string(),
        global_hook_id: global_hook_id.to_string(),
        envs: vec![],
        file_perm: vec![],
        network_perm: vec![],
        state_perm: vec![],
    };
    client.register_hook(Request::new(request)).await
}

/// Registers a sensor.
pub async fn register_sensor(
    controller_addr: &str,
    global_sensor_id: &str,
    keys: Vec<String>,
    tags: Vec<String>,
    app: Vec<u8>,
) -> Result<Response<SensorRegisterResult>, Status> {
    let mut client = KarlControllerClient::connect(controller_addr.to_string())
        .await.map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?;
    let request = SensorRegisterRequest {
        global_sensor_id: global_sensor_id.to_string(),
        keys,
        tags,
        app,
    };
    client.sensor_register(Request::new(request)).await
}

/// Sends a compute request to the given host and returns the result.
pub async fn send_compute(
    host: &str,
    req: ComputeRequest,
) -> Result<Response<NotifyStart>, Status> {
    let mut client = KarlHostClient::connect(host.to_string())
        .await.map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?;
    let request = Request::new(req);
    client.start_compute(request).await
}

/// Push raw data from a sensor.
pub async fn push_raw_data(
    controller_addr: &str,
    sensor_token: SensorToken,
    tag: String,
    data: Vec<u8>,
) -> Result<Response<()>, Status> {
    let mut client = KarlControllerClient::connect(controller_addr.to_string())
        .await.map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?;
    let request = SensorPushData {
        sensor_token,
        tag,
        data,
    };
    debug!("push_raw_data tag={} (len {})", request.tag, request.data.len());
    client.push_raw_data(Request::new(request)).await
}

/*****************************************************************************
 * Service API
 *****************************************************************************/
/// Notify controller about compute request end.
pub async fn notify_end(
    controller_addr: &str, host_token: HostToken, process_token: ProcessToken,
) -> Result<Response<()>, Status> {
    let mut client = KarlControllerClient::connect(controller_addr.to_string())
        .await.map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?;
    let request = NotifyEnd {
        host_token,
        process_token,
    };
    debug!("notify_end host_token={}, process_token={}", request.host_token, request.process_token);
    client.finish_compute(Request::new(request)).await
}

/// Send a heartbeat to the controller.
pub async fn heartbeat(
    controller_addr: &str, host_token: HostToken,
) -> Result<Response<()>, Status> {
    let mut client = KarlControllerClient::connect(controller_addr.to_string())
        .await.map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?;
    let request = HostHeartbeat { host_token };
    client.heartbeat(Request::new(request)).await
}

/// Register a host with the controller.
pub async fn register_host(
    controller_addr: &str, host_id: u32, port: u16, password: &str,
) -> Result<Response<HostRegisterResult>, Status> {
    let mut client = KarlControllerClient::connect(controller_addr.to_string())
        .await.map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?;
    let ip = {
        let socket = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
        socket.connect("8.8.8.8:80").unwrap();
        socket.local_addr().unwrap().ip().to_string()
    };
    let req = HostRegisterRequest {
        host_id: host_id.to_string(),
        ip,
        port: port as _,
        password: password.to_string(),
    };
    info!("Registering host {} at {}:{}", req.host_id, req.ip, req.port);
    client.host_register(Request::new(req)).await
}
