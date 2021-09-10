pub mod protos {
    tonic::include_proto!("request");
}

use tonic::{Request, Status, Code};
use crate::protos::karl_controller_client::KarlControllerClient;
use crate::protos::*;

#[derive(Debug, Clone)]
pub struct KarlSensorSDK {
    pub sensor_id: Option<String>,
    pub controller_addr: String,
    pub sensor_token: Option<String>,
}

impl KarlSensorSDK {
    pub fn new(controller_addr: &str) -> Self {
        Self {
            sensor_id: None,
            controller_addr: controller_addr.to_string(),
            sensor_token: None,
        }
    }

    pub fn new_with_token(controller_addr: &str, token: String) -> Self {
        Self {
            sensor_id: None,
            controller_addr: controller_addr.to_string(),
            sensor_token: Some(token),
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
        KarlControllerClient::connect(self.controller_addr.clone())
            .await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .sensor_register(Request::new(request)).await
            .map(|res| {
                let res = res.into_inner();
                self.sensor_id = Some(res.sensor_id.clone());
                self.sensor_token = Some(res.sensor_token.clone());
                res
            })
    }

    /// Push raw data from a sensor.
    pub async fn push(
        &self,
        output: String,
        data: Vec<u8>,
    ) -> Result<(), Status> {
        let request = SensorPushData {
            sensor_token: self.sensor_token.clone().expect("missing token"),
            output,
            data,
        };
        KarlControllerClient::connect(self.controller_addr.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
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
        KarlControllerClient::connect(self.controller_addr.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .state_changes(Request::new(request)).await
            .map(|res| res.into_inner())
    }
}
