#![feature(proc_macro_hygiene, decl_macro)]
#![feature(custom_inner_attributes)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate rocket;
extern crate rocket_contrib;

mod dashboard;
pub mod net;
pub mod common;
pub mod controller;
mod host;
mod graph;
pub use graph::Graph;
pub mod hook;
pub use controller::Controller;
pub use host::Host;

pub mod protos {
	tonic::include_proto!("request");
}
