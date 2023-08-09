pub mod protos {
    tonic::include_proto!("request");
}

use tonic::{Request, Status, Code};
use tonic::transport::{Channel,Certificate, ClientTlsConfig};
use crate::protos::karl_controller_client::KarlControllerClient;
use crate::protos::*;

#[derive(Debug, Clone)]
pub struct KarlSensorSDK {
    pub controller_addr: String,
    pub sensor_token: Option<String>,
    pub channel: Option<Channel>
}

impl KarlSensorSDK {
    pub fn new(controller_addr: &str) -> Self {
        print!("Hi there");
        Self {
            controller_addr: controller_addr.to_string(),
            sensor_token: None,
            channel: None
        }
    }

    pub fn new_with_token(controller_addr: &str, token: String) -> Self {
        Self {
            controller_addr: controller_addr.to_string(),
            sensor_token: Some(token),
            channel: None
        }
    }

    /// Registers a sensor.
    pub async fn register(
        &mut self,
        global_sensor_id: &str,
        keys: Vec<String>,
        returns: Vec<String>,
        app: Vec<u8>,
    ) -> Result<SensorRegisterResult, Status> {
        let request = SensorRegisterRequest {
            global_sensor_id: global_sensor_id.to_string(),
            keys,
            returns,
            app,
        };

        print!("done with tls configuration!");
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
            .sensor_register(Request::new(request)).await
            .map(|res| {
                let res = res.into_inner();
                self.sensor_token = Some(res.sensor_token.clone());
                res
            })
    }

    /// Push raw data from a sensor.
    pub async fn push(
        &self,
        param: String,
        data: Vec<u8>,
    ) -> Result<(), Status> {
        let request = SensorPushData {
            sensor_token: self.sensor_token.clone().expect("missing token"),
            param,
            data,
        };
        KarlControllerClient::new(self.channel.clone().unwrap())
            .push_raw_data(Request::new(request)).await
            .map(|res| res.into_inner())
    }

    /// Connect to the controller for state changes.
    pub async fn connect_state(
        &self,
    ) -> Result<tonic::Streaming<StateChangePair>, Status> {
        let request = StateChangeInit {
            sensor_token: self.sensor_token.clone().expect("missing token"),
        };
        KarlControllerClient::new(self.channel.clone().unwrap())
            .state_changes(Request::new(request)).await
            .map(|res| res.into_inner())
    }
}
