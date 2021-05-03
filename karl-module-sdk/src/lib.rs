pub mod protos {
    tonic::include_proto!("request");
}

use std::env;
use tonic::{Request, Status};
use crate::protos::karl_host_client::KarlHostClient;
use crate::protos::*;

#[derive(Debug, Clone)]
pub struct KarlModuleSDK {
    pub global_hook_id: String,
    pub hook_id: String,
    pub process_token: String,
    pub host_addr: String,
}

impl KarlModuleSDK {
    pub fn new() -> Self {
        Self {
            global_hook_id: env::var("GLOBAL_HOOK_ID").unwrap(),
            hook_id: env::var("HOOK_ID").unwrap(),
            process_token: env::var("PROCESS_TOKEN").unwrap(),
            host_addr: String::from("http://localhost:59583"),
        }
    }

    /// Returns None if this module was not triggered.
    pub async fn get_triggered(&self) -> Result<Option<Vec<u8>>, Status> {
        let (tag, timestamp) = {
            let tag = env::var("TRIGGERED_TAG").ok();
            let timestamp = env::var("TRIGGERED_TIMESTAMP").ok();
            if tag.is_none() || timestamp.is_none() {
                return Ok(None);
            }
            (tag.unwrap(), timestamp.unwrap())
        };
        self.get(&tag, &timestamp, &timestamp).await.map(|res| Some(res))
    }

    pub async fn get(
        &self,
        tag: &str,
        lower: &str,
        upper: &str,
    ) -> Result<Vec<u8>, Status> {
        let req = GetData {
            host_token: String::new(),
            process_token: self.process_token.clone(),
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
            process_token: self.process_token.clone(),
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
            process_token: self.process_token.clone(),
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
