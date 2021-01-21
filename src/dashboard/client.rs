//! Client specific web API (proxy and storage).
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use std::marker::PhantomData;

use rocket::{
    self, State, Request,
    response::{Debug, NamedFile, Responder},
};
use reqwest::{self, header};

use super::{RequestHeaders, HostHeader, to_client_id};
use crate::controller::Client;
use crate::common::Error;

/// A response with an additional Access-Control-Allow-Origin header
/// corresponding to the client host domain e.g. cam.karl.zapto.org.
pub struct ClientResponse<'a, T: Responder<'a>> {
    response: T,
    host: HostHeader,
    phantom: PhantomData<&'a T>,
}

impl<'r> Responder<'r> for ClientResponse<'_, Option<NamedFile>> {
    fn respond_to(self, req: &Request) -> rocket::response::Result<'r> {
        rocket::response::Response::build_from(self.response.respond_to(req)?)
            .raw_header("Access-Control-Allow-Origin", self.host.0)
            .ok()
    }
}

impl<'r> Responder<'r> for ClientResponse<'_, Result<Vec<u8>, Debug<Error>>> {
    fn respond_to(self, req: &Request) -> rocket::response::Result<'r> {
        rocket::response::Response::build_from(self.response.respond_to(req)?)
            .raw_header("Access-Control-Allow-Origin", self.host.0)
            .ok()
    }
}

/// GET proxy for the url https://<CLIENT_IP>/<PATH>.
#[get("/proxy/<path..>")]
pub fn proxy_get<'a>(
    host_header: HostHeader,
    base_domain: State<String>,
    path: PathBuf,
    clients: State<Arc<Mutex<HashMap<String, Client>>>>,
    headers: RequestHeaders,
) -> ClientResponse<'a, Result<Vec<u8>, Debug<Error>>> {
    ClientResponse {
        response: proxy_get_inner(
            host_header.clone(),
            base_domain,
            path,
            clients,
            headers,
        ),
        host: host_header,
        phantom: PhantomData,
    }
}

fn proxy_get_inner(
    host_header: HostHeader,
    base_domain: State<String>,
    path: PathBuf,
    clients: State<Arc<Mutex<HashMap<String, Client>>>>,
    headers: RequestHeaders,
) -> Result<Vec<u8>, Debug<Error>> {
    let client_id = if let Some(client_id) =
        to_client_id(&host_header, base_domain.to_string()) {
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
pub fn storage<'a>(
    host_header: HostHeader,
    base_domain: State<String>,
    karl_path: State<PathBuf>,
    file: PathBuf,
) -> ClientResponse<'a, Option<NamedFile>> {
    let client_id = if let Some(client_id) =
        to_client_id(&host_header, base_domain.to_string()) {
            client_id
        } else {
            return ClientResponse {
                response: None,
                host: host_header,
                phantom: PhantomData,
            };
        };
    let path = karl_path
        .join("storage")
        .join(client_id)
        .join(file);
    ClientResponse {
        response: NamedFile::open(path).ok(),
        host: host_header,
        phantom: PhantomData
    }
}
