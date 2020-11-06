extern crate protoc_rust;

use protoc_rust::Customize;

fn main() {
    protoc_rust::Codegen::new()
        .out_dir("protos/rust")
        .inputs(&["protos/request.proto"])
        .include("protos")
        .run()
        .expect("protoc");
}
