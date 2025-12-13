fn main() {
    tonic_build::compile_protos("proto/auth_service.proto").unwrap();
    tonic_build::compile_protos("proto/chat_service.proto").unwrap();
}
