fn main() {
    tonic_build::compile_protos("protos/request.proto")
        .expect("tonic_build failed");
}
