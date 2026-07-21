fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use protox to compile proto files without needing protoc installed
    let file_descriptors = protox::compile(["proto/skilluv_ai.proto"], ["proto"])?;

    // tonic 0.14 split codegen into tonic-prost-build; tonic-build stays for the
    // protobuf-agnostic pieces (attributes, transport toggles).
    tonic_prost_build::configure()
        .build_server(false) // We only need the client (server is in Python)
        .build_client(true)
        .out_dir("src/grpc/generated")
        .compile_fds(file_descriptors)?;

    Ok(())
}
