//! Client specific web API (proxy and storage).
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;

use rocket::{
    self, State,
    response::{Debug, NamedFile},
};
use reqwest::{self, header};

use super::{RequestHeaders, HostHeader, to_client_id};
use crate::controller::Client;
use crate::common::Error;

/// GET proxy for the url https://<CLIENT_IP>/<PATH>.
#[get("/proxy/<path..>")]
pub fn proxy_get(
    host_header: HostHeader,
    base_domain: State<String>,
    path: PathBuf,
    clients: State<Arc<Mutex<HashMap<String, Client>>>>,
    headers: RequestHeaders,
) -> Result<Vec<u8>, Debug<Error>> {
    let client_id = if let Some(client_id) =
        to_client_id(host_header, base_domain.to_string()) {
            client_id
        } else {
            return Err(Debug(Error::ProxyError("missing client id".to_string())));
        };
    let ip = clients.lock().unwrap().get(&client_id).ok_or(
        Error::ProxyError(format!("unknown client {:?}", client_id)))?.addr;
    // TODO: HTTPS
    let url = format!("http://{}/{}", ip, path.into_os_string().into_string().unwrap());
    warn!("proxy is NOT using HTTPS!");
    debug!("url={:?}", url);
    debug!("{:?}", headers);
    let mut request = reqwest::blocking::Client::new().get(&url);
    for (key, value) in headers.0 {
        let key = header::HeaderName::from_bytes(key.as_bytes())
            .map_err(|e| Error::ProxyError(format!("{:?}", e)))?;
        let value = header::HeaderValue::from_str(&value)
            .map_err(|e| Error::ProxyError(format!("{:?}", e)))?;
        request = request.header(key, value);
    }
    let response = request.send().map_err(|e| Error::ProxyError(format!("{:?}", e)))?;
    debug!("response = {:?}", response);
    Ok(response.bytes().map_err(|e| Error::ProxyError(format!("{:?}", e)))?.to_vec())
}

#[get("/storage/<file..>")]
pub fn storage(
    host_header: HostHeader,
    base_domain: State<String>,
    karl_path: State<PathBuf>,
    file: PathBuf,
) -> Option<NamedFile> {
    let client_id = if let Some(client_id) =
        to_client_id(host_header, base_domain.to_string()) {
            client_id
        } else {
            return None;
        };
    let path = karl_path
        .join("storage")
        .join(client_id)
        .join(file);
    NamedFile::open(path).ok()
}
