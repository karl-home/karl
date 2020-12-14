//! Controller dashboard.
use rocket;
use tokio::runtime::Runtime;
use std::time::{SystemTime, UNIX_EPOCH};
use rocket_contrib::templates::Template;
use serde::Serialize;

use handlebars::{Helper, Handlebars, Context, RenderContext, Output, HelperResult};

#[derive(Serialize)]
struct TemplateContext {
    title: &'static str,
    hosts: Vec<Host>,
}

type TimeSinceEpoch = u64;

#[derive(Serialize)]
struct Request {
    name: String,
    start: TimeSinceEpoch,
}

impl Request {
    pub fn new(name: String) -> Self {
        Request {
            name,
            start: now(),
        }
    }
}

#[derive(Serialize)]
struct Host {
    id: u32,
    ip: String,
    port: u32,
    is_busy: bool,
    active_request: Option<Request>,
    last_request: Option<Request>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

#[get("/")]
fn index() -> Template {
    Template::render("index", &TemplateContext {
        title: "Hello",
        hosts: vec![Host {
            id: 1,
            ip: "10.0.0.1".to_string(),
            port: 6334,
            is_busy: false,
            active_request: None,
            last_request: Some(Request::new("Wyze".to_string())),
        },
        Host {
            id: 2,
            ip: "10.0.0.1".to_string(),
            port: 6335,
            is_busy: true,
            active_request: Some(Request::new("Wyze".to_string())),
            last_request: Some(Request::new("Wyze".to_string())),
        }],
    })
}

fn request_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output
) -> HelperResult {
    if let Some(param) = h.param(0) {
        if let Some(name) = param.value().get("name") {
            if let Some(name) = name.as_str() {
                out.write(name)?;
            }
        }
        if let Some(start) = param.value().get("start") {
            if let Some(start) = start.as_u64() {
                let current_time = now();
                let elapsed = current_time - start;
                out.write(" (")?;
                out.write(&elapsed.to_string())?;
                out.write("s)")?;
            }
        }
    }

    Ok(())
}

pub fn start(rt: &mut Runtime) {
    rt.spawn(async move {
        rocket::ignite()
        .mount("/", routes![index])
        .attach(Template::custom(|engines| {
            engines.handlebars.register_helper("request", Box::new(request_helper));
        }))
        .launch();
    });
}
