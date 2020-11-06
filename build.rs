extern crate protoc_rust;

fn main() {
	protoc_rust::Codegen::new()
		.out_dir("src/protos")
		.inputs(&["protos/request.proto"])
		.include("protos")
		.run()
		.expect("protoc");
}
