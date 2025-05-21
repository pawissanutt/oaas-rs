use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let mut config = tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .file_descriptor_set_path(out_dir.join("oaas_descriptor.bin"))
        .type_attribute(
            ".",
            "#[cfg_attr(feature = \"serde\", derive(serde::Serialize, serde::Deserialize))]"
        )
        .field_attribute(
            ".oprc.CreateCollectionRequest",
            "#[cfg_attr(feature = \"serde\", serde(default))]"
        )
        .field_attribute(
            ".oprc.InvocationRoute",
            "#[cfg_attr(feature = \"serde\", serde(default))]"
        )
        .field_attribute(
            ".oprc.FuncInvokeRoute",
            "#[cfg_attr(feature = \"serde\", serde(default))]"
        )
        // Use BTreeMap instead of HashMap for all maps in generated code
        .btree_map(&[".oprc.ObjectEvent"])
        .protoc_arg("--experimental_allow_proto3_optional");

    // Enable bytes if the feature is enabled
    if cfg!(feature = "bytes") {
        config = config.bytes(&[".oprc"]);
    }

    config.compile_protos(
        &[
            "proto/oprc-invoke.proto",
            "proto/oprc-route.proto",
            "proto/oprc-data.proto",
        ],
        &["proto/"],
    )?;
    Ok(())
}
