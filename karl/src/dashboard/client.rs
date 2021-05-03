//! Client specific web API (proxy and storage).
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use std::marker::PhantomData;

use serde::Serialize;
use rocket::{
    self, State, Request, http::{Status, Cookie, Cookies},
    response::{NamedFile, Responder},
};
use rocket_contrib::templates::Template;
use reqwest::{self, header};

use super::{RequestHeaders, HostHeader, SessionState, to_client_id};
use karl_common::Client;

const SESSION_COOKIE: &str = "client_session";

#[derive(Serialize)]
struct AppContext {
    client_id: String,
    files: Vec<PathBuf>,
}

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

impl<'r> Responder<'r> for ClientResponse<'_, Result<Vec<u8>, Status>> {
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
    mut cookies: Cookies,
    sessions: State<Arc<Mutex<SessionState>>>,
    path: PathBuf,
    clients: State<Arc<Mutex<HashMap<String, Client>>>>,
    headers: RequestHeaders,
) -> ClientResponse<'a, Result<Vec<u8>, Status>> {
    let mut response = ClientResponse {
        response: Err(Status::InternalServerError),
        host: host_header,
        phantom: PhantomData,
    };
    let client_id = if let Some(client_id) =
        to_client_id(&response.host, base_domain.to_string()) {
            client_id
        } else {
            error!("failed to parse client id: {:?}", &response.host);
            return response;
        };

    // On an authenticated request to a client subdomain, renews the
    // `client_session` cookie associated with the current session. The
    // requester must have an existing cookie that can be obtained by loading
    // resources through the client dashboard.
    if let Some(cookie) = cookies.get_private(SESSION_COOKIE) {
        if !sessions.lock().unwrap().use_cookie(cookie.value(), &client_id) {
            return response;
        }
    } else {
        return response;
    }

    response.response = proxy_get_inner(client_id, path, clients, headers);
    response
}

fn proxy_get_inner(
    client_id: String,
    path: PathBuf,
    clients: State<Arc<Mutex<HashMap<String, Client>>>>,
    headers: RequestHeaders,
) -> Result<Vec<u8>, Status> {
    let ip = clients.lock().unwrap().get(&client_id).ok_or({
        error!("invalid client id but valid session token: {:?}", &client_id);
        Status::InternalServerError
    })?.addr;
    // TODO: HTTPS
    let url = format!("http://{}/{}", ip, path.into_os_string().into_string().unwrap());
    warn!("proxy is NOT using HTTPS!");
    debug!("url={:?}", url);
    debug!("{:?}", headers);
    let mut request = reqwest::blocking::Client::new().get(&url);
    for (key, value) in headers.0 {
        let key = header::HeaderName::from_bytes(key.as_bytes()).map_err(|e| {
            error!("failed to parse headers: {}", e);
            Status::BadRequest
        })?;
        let value = header::HeaderValue::from_str(&value).map_err(|e| {
            error!("failed to parse headers: {}", e);
            Status::BadRequest
        })?;
        request = request.header(key, value);
    }
    let response = request.send().map_err(|e| {
        error!("failed to forward request: {}", e);
        Status::BadRequest
    })?;
    debug!("response = {:?}", response);
    Ok(response.bytes().map_err(|e| {
        error!("failed to parse response: {}", e);
        Status::BadRequest
    })?.to_vec())
}

#[get("/storage/<file..>")]
pub fn storage<'a>(
    host_header: HostHeader,
    base_domain: State<String>,
    mut cookies: Cookies,
    sessions: State<Arc<Mutex<SessionState>>>,
    karl_path: State<PathBuf>,
    file: PathBuf,
) -> ClientResponse<'a, Option<NamedFile>> {
    let mut response = ClientResponse {
        response: None,
        host: host_header,
        phantom: PhantomData,
    };

    let client_id = if let Some(client_id) =
        to_client_id(&response.host, base_domain.to_string()) {
            client_id
        } else {
            error!("failed to parse client id: {:?}", &response.host);
            return response;
        };

    // On an authenticated request to a client subdomain, renews the
    // `client_session` cookie associated with the current session. The
    // requester must have an existing cookie that can be obtained by loading
    // resources through the client dashboard.
    if let Some(cookie) = cookies.get_private(SESSION_COOKIE) {
        if !sessions.lock().unwrap().use_cookie(cookie.value(), &client_id) {
            return response;
        }
    } else {
        return response;
    }

    let path = karl_path
        .join("storage")
        .join(client_id)
        .join(file);
    response.response = NamedFile::open(path).ok();
    response
}

pub fn index(
    client_id: String,
    karl_path: State<PathBuf>,
    mut cookies: Cookies,
    sessions: State<Arc<Mutex<SessionState>>>,
) -> Option<Template> {
    // On an authenticated request to a client subdomain, creates a
    // new cookie `client_session` for the session or renews the cookie
    // associated  the session. An existing cookie is not required to
    // retrieve the client dashboard.
    {
        let mut sessions = sessions.lock().unwrap();
        if let Some(cookie) = cookies.get_private(SESSION_COOKIE) {
            if !sessions.use_cookie(cookie.value(), &client_id) {
                // cookie expired
                cookies.add_private(Cookie::new(
                    SESSION_COOKIE,
                    sessions.gen_cookie(&client_id),
                ));
            }
        } else {
            cookies.add_private(Cookie::new(
                SESSION_COOKIE,
                sessions.gen_cookie(&client_id),
            ));
        }
    }

    let files = {
        let storage_path = karl_path.join("storage").join(&client_id);
        let files = std::fs::read_dir(&storage_path).ok();
        let mut files = if let Some(files) = files {
            files
            .map(|res| res.unwrap().path())
            .map(|path| path.strip_prefix(&storage_path).unwrap().to_path_buf())
            .collect::<Vec<_>>()
        } else {
            // client does not exist
            return None;
        };
        files.sort();
        files
    };
    debug!("files for client_id={}: {:?}", &client_id, files);
    Some(Template::render(
        client_id.clone(),
        &AppContext { client_id, files },
    ))
}
