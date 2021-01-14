//! Proxy and storage API.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;

use rocket::{
    self, State,
    response::NamedFile,
    request::{FromRequest, Outcome},
};
use reqwest::{self, header};
use serde::Serialize;

use crate::controller::Client;
use crate::common::Error;

#[derive(Serialize, Debug)]
pub struct RequestHeaders(Vec<(String, String)>);

impl<'a, 'r> FromRequest<'a, 'r> for RequestHeaders {
    type Error = Error;

    fn from_request(request: &'a rocket::Request<'r>) -> Outcome<Self, Self::Error> {
        let headers = request
            .headers()
            .iter()
            .map(|header| (header.name().to_string(), header.value().to_string()))
            .collect();
        Outcome::Success(RequestHeaders(headers))
    }
}

/// GET proxy for the url https://<CLIENT_IP>/<PATH>.
#[get("/proxy/<client_id>/<path..>")]
pub fn proxy_get(
    client_id: String,
    path: PathBuf,
    clients: State<Arc<Mutex<HashMap<String, Client>>>>,
    headers: RequestHeaders,
) -> Result<Vec<u8>, rocket::response::Debug<Error>> {
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

#[get("/storage/<client_id>/<file..>")]
pub fn storage(
    karl_path: State<PathBuf>,
    client_id: String,
    file: PathBuf,
) -> Option<NamedFile> {
    let path = karl_path
        .join("storage")
        .join(client_id)
        .join(file);
    NamedFile::open(path).ok()
}
