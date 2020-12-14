//! Controller dashboard.
use rocket;
use tokio::runtime::Runtime;
use rocket_contrib::templates::Template;
use serde::Serialize;

use handlebars::{Helper, Handlebars, Context, RenderContext, Output, HelperResult};
use crate::controller::{Request, Host};

#[derive(Serialize)]
struct TemplateContext {
    title: &'static str,
    hosts: Vec<Host>,
}

#[get("/")]
fn index() -> Template {
    Template::render("index", &TemplateContext {
        title: "Hello",
        hosts: vec![Host {
            index: 1,
            name: "1".to_string(),
            addr: "10.0.0.1:6334".parse().unwrap(),
            is_busy: false,
            active_request: None,
            last_request: Some(Request::new("Wyze".to_string())),
        },
        Host {
            index: 2,
            name: "2".to_string(),
            addr: "10.0.0.1:6335".parse().unwrap(),
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
                let current_time = Request::time_since_epoch_s();
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
