//! Packages listing (mock/dev or proxy to PM later)

use crate::types::PackagesSnapshot;
use dioxus::prelude::*;

#[post("/api/proxy/packages")]
pub async fn proxy_packages() -> Result<PackagesSnapshot, ServerFnError> {
    #[cfg(not(feature = "server"))]
    {
        // This code is never reached on client - replaced by HTTP stub
        unreachable!()
    }

    #[cfg(feature = "server")]
    {
        if crate::config::is_dev_mock() {
            Ok(crate::api::mock::mock_packages_snapshot())
        } else {
            // Call Package Manager REST API
            let base = crate::config::pm_base_url();
            let url = format!("{}/api/v1/packages", base);
            let client = reqwest::Client::new();
            let resp = client
                .get(&url)
                .header("Accept", "application/json")
                .send()
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;

            let status = resp.status();
            if !status.is_success() {
                return Err(ServerFnError::new(format!(
                    "PM packages request failed: {}",
                    status
                )));
            }

            // Deserialize into Vec<OPackage>
            let pkgs: Vec<oprc_models::OPackage> = resp
                .json()
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;

            // Map to UI snapshot structures
            let mapped: Vec<crate::types::PackageInfo> = pkgs
                .into_iter()
                .map(|p| {
                    // Build class infos grouping stateless/stateful bindings
                    let mut class_infos: Vec<crate::types::PackageClassInfo> =
                        Vec::new();
                    for class in &p.classes {
                        let mut stateless = Vec::new();
                        let mut stateful = Vec::new();
                        for binding in &class.function_bindings {
                            if binding.stateless {
                                stateless.push(binding.function_key.clone());
                            } else {
                                stateful.push(binding.function_key.clone());
                            }
                        }
                        class_infos.push(crate::types::PackageClassInfo {
                            key: class.key.clone(),
                            description: class.description.clone(),
                            stateless_functions: stateless,
                            stateful_functions: stateful,
                        });
                    }

                    // Map top-level functions
                    let functions: Vec<crate::types::PackageFunctionInfo> = p
                        .functions
                        .into_iter()
                        .map(|f| crate::types::PackageFunctionInfo {
                            key: f.key,
                            function_type: format!("{:?}", f.function_type)
                                .to_lowercase(),
                            description: f.description,
                        })
                        .collect();

                    crate::types::PackageInfo {
                        name: p.name,
                        version: p.version,
                        classes: class_infos,
                        functions,
                        dependencies: p.dependencies,
                    }
                })
                .collect();

            Ok(crate::types::PackagesSnapshot {
                packages: mapped,
                timestamp: chrono::Utc::now().to_rfc3339(),
            })
        }
    }
}
