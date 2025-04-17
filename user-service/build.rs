fn main() {
    tonic_build::compile_protos("proto/matcher.proto").unwrap();
}
