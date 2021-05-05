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
        let params = if let Some(params) = env::var("KARL_PARAMS").ok() {
            params
            .split(":")
            .map(|param| param.split(";"))
            .filter_map(|mut split| {
                if let Some(param) = split.next() {
                    if let Some(tag) = split.next() {
                        Some((param, tag))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .map(|(param, tag)| (param.to_string(), tag.to_string()))
            .collect::<HashMap<_, _>>()
        } else {
            HashMap::new()
        };
        let returns = if let Some(returns) = env::var("KARL_RETURNS").ok() {
            returns
            .split(":")
            .map(|param| param.split(";"))
            .filter_map(|mut split| {
                if let Some(return_name) = split.next() {
                    if let Some(tags) = split.next() {
                        Some((return_name, tags))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .map(|(return_name, tags)| (
                return_name.to_string(),
                tags.split(",").map(|tag| tag.to_string()).collect(),
            ))
            .collect::<HashMap<_, _>>()
        } else {
            HashMap::new()
        };
        println!("params: {:?}", params);
        println!("params: {:?}", returns);
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
            self._get_tag(tag, lower, upper).await
        } else {
            println!("{:?}", param);
            println!("{:?}", self.params);
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
            let mut reqs = (0..(tags.len() - 1))
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
            println!("{:?}", return_name);
            println!("{:?}", self.returns);
            Err(Status::new(Code::Cancelled, "invalid return: either the \
                user did not create an output edge from the return OR the \
                module incorrectly registered its expected returns. \
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
