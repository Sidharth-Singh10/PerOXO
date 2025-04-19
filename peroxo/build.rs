fn main() {
    tonic_build::compile_protos("proto/user_service.proto").unwrap();
}
