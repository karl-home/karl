extern crate protoc_rust;

fn main() {
    protoc_rust::Codegen::new()
        .out_dir("protos/rust")
        .inputs(&["protos/request.proto"])
        .include("protos")
        .run()
        .expect("protoc failed");
}
