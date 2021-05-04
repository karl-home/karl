pub mod protos {
    tonic::include_proto!("request");
}

use std::env;
use std::collections::HashMap;
use tonic::{Request, Status, Code};
use crate::protos::karl_host_client::KarlHostClient;
use crate::protos::*;

#[derive(Debug, Clone)]
pub struct KarlModuleSDK {
    pub global_hook_id: String,
    pub hook_id: String,
    pub process_token: String,
    pub host_addr: String,
    /// Map from registered parameter to input tag
    pub params: HashMap<String, String>,
    /// Map from registered returns to output tag(s)
    pub returns: HashMap<String, Vec<String>>,
}

impl KarlModuleSDK {
    pub fn new() -> Self {
        let params = env::var("KARL_PARAMS").unwrap()
            .split(":")
            .map(|param| param.split(";"))
            .map(|param| (param.next().unwrap(), param.next().unwrap()))
            .map(|(param, tag)| (param.to_string(), tag.to_string()))
            .collect::HashMap<_, _>();
        let returns = env::var("KARL_RETURNS").unwrap()
            .split(":")
            .map(|param| param.split(";"))
            .map(|param| (param.next().unwrap(), param.next().unwrap()))
            .map(|(return_name, tags)| (
                return_name.to_string(),
                tags.split(",").map(|tag| tag.to_string()).collect(),
            ))
            .collect::HashMap<_, _>();
        Self {
            global_hook_id: env::var("GLOBAL_HOOK_ID").unwrap(),
            hook_id: env::var("HOOK_ID").unwrap(),
            process_token: env::var("PROCESS_TOKEN").unwrap(),
            host_addr: String::from("http://localhost:59583"),
            params,
            returns,
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
        Ok(self._get_tag(&tag, &timestamp, &timestamp).await?.data.pop())
    }

    pub async fn get(
        &self,
        param: &str,
        lower: &str,
        upper: &str,
    ) -> Result<GetDataResult, Status> {
        if let Some(tag) = self.params.get(param) {
            self._get_tag(tag, lower, upper)
        } else {
            Err(Status::new(Code::Cancelled, "invalid param: either the \
                user did not create an input edge to the param OR the \
                module incorrectly registered its expected params. \
                The validation will also fail on the host if allowed \
                to proceed."))
        }
    }

    async fn _get_tag(
        &self,
        tag: &str,
        lower: &str,
        upper: &str,
    ) -> Result<GetDataResult, Status> {
        let req = GetData {
            host_token: String::new(),
            process_token: self.process_token.clone(),
            tag: tag.to_string(),
            lower: lower.to_string(),
            upper: upper.to_string(),
        };
        KarlHostClient::connect(self.host_addr.clone()).await.unwrap()
            .get(Request::new(req)).await
            .map(|res| res.into_inner())
    }

    /// Push data to ALL tags for the return name.
    pub async fn push(
        &self,
        return_name: &str,
        data: Vec<u8>,
    ) -> Result<(), Status> {
        if let Some(tags) = self.returns.get(return_name) {
            assert!(!tags.is_empty());
            let mut reqs = 0..(tags.len() - 1)
                .iter()
                .map(|i| PushData {
                    host_token: String::new(),
                    process_token: self.process_token.clone(),
                    tag: tags.get(i).unwrap().clone(),
                    data: data.clone(),
                })
                .collect::<Vec<_>>();
            // avoid cloning data unnecessarily
            reqs.push(PushData {
                host_token: String::new(),
                process_token: self.process_token.clone(),
                tag: tags.get(tags.len() - 1).unwrap().clone(),
                data,
            });
            for req in reqs {
                KarlHostClient::connect(self.host_addr.clone()).await.unwrap()
                    .push(Request::new(req)).await
                    .map(|res| res.into_inner())?;
            }
            Ok(())
        } else {
            Err(Status::new(Code::Cancelled, "invalid param: either the \
                user did not create an input edge to the param OR the \
                module incorrectly registered its expected params. \
                The validation will also fail on the host if allowed \
                to proceed."))
        }
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
