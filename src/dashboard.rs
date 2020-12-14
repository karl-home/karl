//! Controller dashboard.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rocket;
use rocket::State;
use rocket_contrib::templates::Template;
use tokio::runtime::Runtime;
use serde::Serialize;

use handlebars::{Helper, Handlebars, Context, RenderContext, Output, HelperResult};
use crate::controller::{Request, Host};

#[derive(Serialize)]
struct TemplateContext {
    title: &'static str,
    hosts: Vec<Host>,
}

#[get("/")]
fn index(hosts: State<Arc<Mutex<HashMap<String, Host>>>>) -> Template {
    Template::render("index", &TemplateContext {
        title: "Hello",
        hosts: hosts.lock().unwrap().values().map(|host| host.clone()).collect(),
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

pub fn start(rt: &mut Runtime, hosts: Arc<Mutex<HashMap<String, Host>>>) {
    rt.spawn(async move {
        rocket::ignite()
        .manage(hosts)
        .mount("/", routes![index])
        .attach(Template::custom(|engines| {
            engines.handlebars.register_helper("request", Box::new(request_helper));
        }))
        .launch();
    });
}
