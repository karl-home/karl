/// Helper functions and structs for the webserver.
use serde::Serialize;
use rocket::request::{FromRequest, Outcome};
use handlebars::{Helper, Handlebars, Context, RenderContext, Output, HelperResult};
use crate::common::Error;

#[derive(Serialize, Debug)]
pub struct RequestHeaders(pub Vec<(String, String)>);
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

/// Host who sent the request.
#[derive(Serialize, Debug, Clone)]
pub struct HostHeader(pub String);
impl<'a, 'r> FromRequest<'a, 'r> for HostHeader {
    type Error = ();

    fn from_request(request: &'a rocket::Request<'r>) -> Outcome<Self, Self::Error> {
        match request.headers().get_one("Host") {
            Some(h) => Outcome::Success(HostHeader(h.to_string())),
            None => Outcome::Forward(()),
        }
    }
}

/// Convert a host header to a client ID based on the base domain
/// provided when the controller started.
pub fn to_client_id(
    header: &HostHeader,
    base_domain: String,
) -> Option<String> {
    if let Some(subdomain) = header.0.strip_suffix(&base_domain) {
        let subdomain = subdomain.trim_end_matches(".");
        if subdomain.len() > 0 {
            Some(subdomain.to_string())
        } else {
            None
        }
    } else {
        None
    }
}

/// Handlebars helper function for rendering requests in the main dashboard.
pub fn request_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output
) -> HelperResult {
    if let Some(param) = h.param(0) {
        if let Some(description) = param.value().get("description") {
            if let Some(description) = description.as_str() {
                out.write(description)?;
            }
        }
        if let Some(time) = param.value().get("time") {
            if let Some(time) = time.as_f64() {
                out.write(" (")?;
                out.write(&format!("{:.3}", time))?;
                out.write("s)")?;
            }
        }
    }

    Ok(())
}
