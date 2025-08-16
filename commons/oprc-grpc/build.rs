use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
    // Enable proto3 optional field semantics (required for fields declared with 'optional').
    .protoc_arg("--experimental_allow_proto3_optional")
        .file_descriptor_set_path(out_dir.join("shared_modules_descriptor.bin"))
        .type_attribute(
            ".",
            "#[cfg_attr(feature = \"serde\", derive(serde::Serialize, serde::Deserialize))]"
        )
        .compile_protos(
            &[
                "proto/common.proto",
                "proto/package.proto", 
                "proto/deployment.proto",
                "proto/runtime.proto",
                "proto/health.proto",
            ],
            &["proto/"],
        )?;

    Ok(())
}
