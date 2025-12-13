fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        // generated code will be placed in OUT_DIR and included via `tonic::include_proto!`
        .compile(&["proto/auth_service.proto"], &["proto"])?;

    Ok(())
}
