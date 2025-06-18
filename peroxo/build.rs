fn main() {
    tonic_build::compile_protos("proto/user_service.proto").unwrap();
    tonic_build::compile_protos("proto/chat_service.proto").unwrap();
}
