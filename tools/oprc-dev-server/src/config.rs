use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;

/// Top-level dev server configuration (oaas-dev.yaml).
#[derive(Debug, Deserialize)]
pub struct DevServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub collections: Vec<CollectionConfig>,
}

/// A single OaaS collection to serve locally.
#[derive(Debug, Deserialize)]
pub struct CollectionConfig {
    pub name: String,
    #[serde(default)]
    pub partition_id: u32,
    #[serde(default)]
    pub functions: Vec<FunctionConfig>,
}

/// A function (WASM component) attached to a collection.
#[derive(Debug, Deserialize)]
pub struct FunctionConfig {
    pub id: String,
    pub wasm: PathBuf,
}

fn default_port() -> u16 {
    8080
}

impl DevServerConfig {
    /// Load configuration from a YAML file.
    pub fn from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: Self = serde_yaml::from_str(&contents)?;
        Ok(config)
    }
}

impl CollectionConfig {
    /// Convert to a `CreateCollectionRequest` for ODGM.
    pub fn to_create_request(&self) -> oprc_grpc::CreateCollectionRequest {
        let mut fn_routes = HashMap::new();
        for func in &self.functions {
            let wasm_path = self.resolve_wasm_path(&func.wasm);
            fn_routes.insert(
                func.id.clone(),
                oprc_grpc::FuncInvokeRoute {
                    url: format!("wasm://{}", func.id),
                    wasm_module_url: Some(wasm_path),
                    stateless: false,
                    standby: false,
                    active_group: vec![],
                },
            );
        }

        let invocations = if fn_routes.is_empty() {
            None
        } else {
            Some(oprc_grpc::InvocationRoute {
                fn_routes,
                disabled_fn: vec![],
            })
        };

        oprc_grpc::CreateCollectionRequest {
            name: self.name.clone(),
            partition_count: 1,
            replica_count: 1,
            shard_assignments: vec![],
            shard_type: "none".to_string(),
            options: HashMap::new(),
            invocations,
        }
    }

    fn resolve_wasm_path(&self, path: &std::path::Path) -> String {
        // Convert relative path to absolute
        if path.is_absolute() {
            format!("file://{}", path.display())
        } else {
            let abs = std::env::current_dir().unwrap_or_default().join(path);
            format!("file://{}", abs.display())
        }
    }
}
