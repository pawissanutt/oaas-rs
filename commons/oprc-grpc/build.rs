use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Only build gRPC client/server when grpc feature is enabled
    let build_grpc = cfg!(feature = "grpc");

    let mut config = tonic_prost_build::configure()
        .build_server(build_grpc)
        .build_client(build_grpc)
        .protoc_arg("--experimental_allow_proto3_optional")
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
        .field_attribute(
            ".oprc.TriggerPayload",
            "#[cfg_attr(feature = \"serde\", serde(default))]"
        )
        .field_attribute(
            ".oprc.EventInfo",
            "#[cfg_attr(feature = \"serde\", serde(default))]"
        )
        .field_attribute(
            ".oprc.TriggerInfo",
            "#[cfg_attr(feature = \"serde\", serde(default))]"
        )
        .btree_map(".oprc.ObjectEvent")
        .btree_map(".oprc.TriggerPayload")
        .btree_map(".oprc.EventInfo")
        .btree_map(".oprc.TriggerInfo");

    if cfg!(feature = "bytes") {
        config = config.bytes(".oprc");
    }

    config.compile_protos(
        &[
            "proto/common.proto",
            "proto/package.proto",
            "proto/deployment_types.proto",
            "proto/deployment.proto",
            "proto/runtime.proto",
            "proto/health.proto",
            "proto/topology.proto",
            "proto/oprc-invoke.proto",
            "proto/oprc-route.proto",
            "proto/oprc-data.proto",
        ],
        &["proto/"],
    )?;

    Ok(())
}
