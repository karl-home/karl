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
use std::env;
use tonic::{Request, Response, Status, Code};
use crate::protos::karl_controller_client::KarlControllerClient;
use crate::protos::karl_host_client::KarlHostClient;
use crate::protos::*;
use crate::common::*;

pub struct KarlAPI {
    pub global_hook_id: String,
    pub hook_id: String,
    pub token: String,
    pub host_addr: String,
}

impl KarlAPI {
    pub fn new() -> Self {
        Self {
            global_hook_id: env::var("GLOBAL_HOOK_ID").unwrap(),
            hook_id: env::var("HOOK_ID").unwrap(),
            token: env::var("PROCESS_TOKEN").unwrap(),
            host_addr: String::from("http://localhost:59583"),
        }
    }

    pub async fn get(
        &self,
        tag: &str,
        lower: &str,
        upper: &str,
    ) -> Result<Vec<u8>, Status> {
        let req = GetData {
            host_token: String::new(),
            process_token: self.token.clone(),
            tag: tag.to_string(),
            lower: lower.to_string(),
            upper: upper.to_string(),
        };
        KarlHostClient::connect(self.host_addr.clone()).await.unwrap()
            .get(Request::new(req)).await
            .map(|res| res.into_inner().data)
    }

    pub async fn push(
        &self,
        tag: &str,
        data: Vec<u8>,
    ) -> Result<(), Status> {
        let req = PushData {
            host_token: String::new(),
            process_token: self.token.clone(),
            tag: format!("{}.{}", self.hook_id, tag),
            data,
        };
        KarlHostClient::connect(self.host_addr.clone()).await.unwrap()
            .push(Request::new(req)).await
            .map(|res| res.into_inner())
    }

    pub async fn network(
        &self,
        domain: &str,
        method: &str,
        headers: Vec<(Vec<u8>, Vec<u8>)>,
        body: Vec<u8>,
    ) -> Result<NetworkAccessResult, Status> {
        let headers = headers
            .into_iter()
            .map(|(key, value)| { KeyValuePair { key, value } })
            .collect();
        let req = NetworkAccess {
            host_token: String::new(),
            process_token: self.token.clone(),
            domain: domain.to_string(),
            method: method.to_string(),
            headers,
            body,
        };
        KarlHostClient::connect(self.host_addr.clone()).await.unwrap()
            .network(Request::new(req)).await
            .map(|res| res.into_inner())
    }
}

/// Registers a hook.
pub async fn register_hook(
    controller_addr: &str,
    global_hook_id: &str,
) -> Result<Response<RegisterHookResult>, Status> {
    let mut client = KarlControllerClient::connect(controller_addr.to_string())
        .await.map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?;
    let request = RegisterHookRequest {
        token: "".to_string(),
        global_hook_id: global_hook_id.to_string(),
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

/// Adds data edge.
pub async fn add_data_edge(
    controller_addr: &str,
    output_id: String,
    output_tag: String,
    input_id: String,
    trigger: bool,
) -> Result<Response<()>, Status> {
    let mut client = KarlControllerClient::connect(controller_addr.to_string())
        .await.map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?;
    let request = AddDataEdgeRequest {
        output_id,
        output_tag,
        input_id,
        trigger,
    };
    client.add_data_edge(Request::new(request)).await
}

/// Adds network edge.
pub async fn add_network_edge(
    controller_addr: &str,
    output_id: String,
    domain: String,
) -> Result<Response<()>, Status> {
    let mut client = KarlControllerClient::connect(controller_addr.to_string())
        .await.map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?;
    let request = AddNetworkEdgeRequest {
        output_id,
        domain,
    };
    client.add_network_edge(Request::new(request)).await
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
