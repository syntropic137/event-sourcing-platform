fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Ensure a working protoc is available even without system install
    let protoc_path = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc_path);

    let proto_files = &["proto/eventstore/v1/eventstore.proto"];
    let include_dirs = &["proto"]; // import roots

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(proto_files, include_dirs)?;

    println!("cargo:rerun-if-changed=proto/eventstore/v1/eventstore.proto");
    println!("cargo:rerun-if-changed=proto");
    Ok(())
}
