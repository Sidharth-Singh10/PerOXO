fn main() {
    // tonic-build 0.13 delegates proto compilation to prost-build and exposes
    // the same one-shot helper the other two services use.
    tonic_build::compile_protos("proto/auth_service.proto").unwrap();
}
