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
use tonic::transport::{ClientTlsConfig, Channel, Certificate};
use crate::protos::karl_controller_client::KarlControllerClient;
use crate::protos::*;
use karl_common::*;

#[derive(Debug, Clone)]
pub struct KarlHostAPI {
    pub controller_addr: String,
    /// Assigned after registering with controller
    pub host_token: Option<String>,
    pub channel: Option<Channel> 
}

impl KarlHostAPI {
    pub fn new(controller_addr: &str) -> Self {
        Self {
            controller_addr: controller_addr.to_string(),
            host_token: None,
            channel: None 
        }
    }

    /// Register a host with the controller.
    pub async fn register(
        &mut self,
        host_id: u32,
        port: u16,
        password: &str,
    ) -> Result<HostRegisterResult, Status> {
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

        let pem = tokio::fs::read("../ca.pem").await.unwrap();
        let ca = Certificate::from_pem(pem);

        let tls = ClientTlsConfig::new()
            .domain_name("localhost")
            .ca_certificate(ca);
        
        self.channel = Some(Channel::from_shared(self.controller_addr.to_owned()).unwrap()
            .tls_config(tls).unwrap()
            .connect()
            .await.unwrap());

        KarlControllerClient::new(self.channel.clone().unwrap())
            .host_register(Request::new(req)).await
            .map(|res| {
                let res = res.into_inner();
                self.host_token = Some(res.host_token.clone());
                res
            })
    }

    /// Notify controller about compute request end.
    pub async fn notify_end(
        &self,
        process_token: ProcessToken,
    ) -> Result<(), Status> {
        let request = NotifyEnd {
            host_token: self.host_token.clone().expect("missing token"),
            process_token,
        };
        trace!("notify_end host_token={}, process_token={}", request.host_token, request.process_token);
        KarlControllerClient::new(self.channel.clone().unwrap())
            .finish_compute(Request::new(request)).await
            .map(|res| res.into_inner())
    }

    /// Send a heartbeat to the controller.
    pub async fn heartbeat(&self) -> Result<(), Status> {
        let request = HostHeartbeat {
            host_token: self.host_token.clone().expect("missing token"),
        };

        KarlControllerClient::new(self.channel.clone().unwrap())
            .heartbeat(Request::new(request)).await
            .map(|res| res.into_inner())
    }

    /// Forward a network request to the controller.
    pub async fn forward_network(
        &self,
        mut req: NetworkAccess,
    ) -> Result<Response<()>, Status> {
        req.host_token = self.host_token.clone().expect("missing token");
        KarlControllerClient::new(self.channel.clone().unwrap())
            .forward_network(Request::new(req)).await
    }

    pub async fn forward_get(
        &self,
        mut req: GetData,
    ) -> Result<Response<GetDataResult>, Status> {
        req.host_token = self.host_token.clone().expect("missing token");
        KarlControllerClient::new(self.channel.clone().unwrap())
            .forward_get(Request::new(req)).await
    }

    pub async fn forward_push(
        &self,
        mut req: PushData,
    ) -> Result<Response<()>, Status> {
        req.host_token = self.host_token.clone().expect("missing token");
        KarlControllerClient::new(self.channel.clone().unwrap())
            .forward_push(Request::new(req)).await
    }

    pub async fn forward_state(
        &self,
        mut req: StateChange,
    ) -> Result<Response<()>, Status> {
        req.host_token = self.host_token.clone().expect("missing token");
        KarlControllerClient::new(self.channel.clone().unwrap())
            .forward_state(Request::new(req)).await
    }
}
