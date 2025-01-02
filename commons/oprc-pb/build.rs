use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .file_descriptor_set_path(out_dir.join("oaas_descriptor.bin"))
        .type_attribute(".", "#[derive(serde::Serialize,serde::Deserialize)]")
        .bytes(&[".oprc"])
        .protoc_arg("--experimental_allow_proto3_optional")
        // .btree_map(&[".oprc.ObjData"])
        .compile_protos(
            &[
                "proto/oprc-invoke.proto",
                "proto/oprc-route.proto",
                "proto/oprc-data.proto",
            ],
            &["proto/"],
        )?;
    Ok(())
}
