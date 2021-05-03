#![feature(proc_macro_hygiene, decl_macro)]
#![feature(custom_inner_attributes)]

#[macro_use]
extern crate log;

pub mod net;
pub mod common;
mod host;
pub mod hook;
pub use host::Host;

pub mod protos {
	tonic::include_proto!("request");
}
