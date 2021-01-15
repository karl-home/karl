#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate rocket;
extern crate rocket_contrib;
#[cfg(target_os = "linux")]
extern crate sys_mount;
#[cfg(test)]
extern crate serial_test;

mod dashboard;
pub mod protos;
pub mod net;
pub mod packet;
pub mod backend;
pub mod common;
mod controller;
mod host;
pub use controller::Controller;
pub use host::Host;
