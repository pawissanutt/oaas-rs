//! Script service — orchestrates compile → store artifact → store source →
//! create/update package → create deployment.

use crate::{
    errors::{ApiError, PackageManagerError},
    services::{
        DeploymentService, PackageService,
        artifact::{ArtifactError, ArtifactMeta, ArtifactStore, SourceStore},
        compiler::{CompilerClient, CompilerError},
    },
};
use oprc_models::{
    DeploymentCondition, FunctionAccessModifier, FunctionBinding, FunctionType,
    OClass, OClassDeployment, OFunction, OPackage, ProvisionConfig,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};

/// Request to compile a script (validation only, no storage).
#[derive(Debug, Deserialize)]
pub struct CompileRequest {
    pub source: String,
    #[serde(default = "default_language")]
    pub language: String,
}

/// Response from compile-only request.
#[derive(Debug, Serialize)]
pub struct CompileResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    /// Size of the compiled WASM module in bytes (only on success).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wasm_size: Option<usize>,
}

/// Request to compile and deploy a script.
#[derive(Debug, Deserialize)]
pub struct DeployScriptRequest {
    pub source: String,
    #[serde(default = "default_language")]
    pub language: String,
    pub package_name: String,
    pub class_key: String,
    /// Function bindings: maps method names to function keys.
    /// If empty, the compiler/SDK extracts methods automatically.
    #[serde(default)]
    pub function_bindings: Vec<ScriptFunctionBinding>,
    /// Target environments to deploy to. If empty, system selects automatically.
    #[serde(default)]
    pub target_envs: Vec<String>,
}

/// A function binding extracted from the script or specified by the user.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ScriptFunctionBinding {
    pub name: String,
    #[serde(default)]
    pub stateless: bool,
}

/// Response from the deploy endpoint.
#[derive(Debug, Serialize)]
pub struct DeployScriptResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_url: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Response for GET /api/v1/scripts/{package}/{function}
#[derive(Debug, Serialize)]
pub struct ScriptSourceResponse {
    pub package: String,
    pub function: String,
    pub source: String,
    pub language: String,
}

/// Request to build a script (compile + store, no package/deployment).
#[derive(Debug, Deserialize)]
pub struct BuildScriptRequest {
    pub source: String,
    #[serde(default = "default_language")]
    pub language: String,
    /// Package name for source storage key.
    pub package_name: String,
    /// Class key for source storage key.
    pub class_key: String,
}

/// Response from the build endpoint.
#[derive(Debug, Serialize)]
pub struct BuildScriptResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wasm_size: Option<usize>,
    #[serde(default)]
    pub source_stored: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Response for GET /api/v1/artifacts — enriched artifact info.
#[derive(Debug, Serialize)]
pub struct ArtifactListEntry {
    pub id: String,
    pub size: u64,
    pub url: String,
    pub has_source: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_package: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_function: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub built_at: Option<String>,
}

/// Request to test a script method using the compiler service's real SDK runtime.
#[derive(Debug, Deserialize)]
pub struct TestScriptRequest {
    pub source: String,
    #[serde(default = "default_language")]
    pub language: String,
    pub method_name: String,
    pub payload: Option<serde_json::Value>,
    #[serde(default)]
    pub initial_state: Option<serde_json::Value>,
}

fn default_language() -> String {
    "typescript".to_string()
}

/// Orchestrates script compilation, artifact storage, and deployment.
pub struct ScriptService {
    compiler: Arc<CompilerClient>,
    artifact_store: Arc<dyn ArtifactStore>,
    source_store: Arc<dyn SourceStore>,
    package_service: Arc<PackageService>,
    deployment_service: Arc<DeploymentService>,
    /// Base URL for serving artifacts (e.g., `http://oprc-pm:8080/api/v1/artifacts`).
    artifact_base_url: String,
}

impl ScriptService {
    pub fn new(
        compiler: Arc<CompilerClient>,
        artifact_store: Arc<dyn ArtifactStore>,
        source_store: Arc<dyn SourceStore>,
        package_service: Arc<PackageService>,
        deployment_service: Arc<DeploymentService>,
        artifact_base_url: String,
    ) -> Self {
        Self {
            compiler,
            artifact_store,
            source_store,
            package_service,
            deployment_service,
            artifact_base_url,
        }
    }

    /// Compile a script without storing — validation only.
    pub async fn compile_script(
        &self,
        req: &CompileRequest,
    ) -> CompileResponse {
        match self.compiler.compile(&req.source, &req.language).await {
            Ok(wasm_bytes) => CompileResponse {
                success: true,
                errors: vec![],
                wasm_size: Some(wasm_bytes.len()),
            },
            Err(CompilerError::CompilationFailed(errors)) => CompileResponse {
                success: false,
                errors,
                wasm_size: None,
            },
            Err(e) => CompileResponse {
                success: false,
                errors: vec![format!("Compiler error: {}", e)],
                wasm_size: None,
            },
        }
    }

    /// Test a script method using the compiler service's real SDK runtime.
    ///
    /// Forwards the request to the compiler's `/test` endpoint, which bundles
    /// the code with the real `@oaas/sdk` via esbuild and runs it in Node.js.
    pub async fn test_script(
        &self,
        req: &TestScriptRequest,
    ) -> Result<crate::services::compiler::TestScriptResponse, ApiError> {
        let response = self
            .compiler
            .test_script(
                &req.source,
                &req.method_name,
                req.payload.clone(),
                req.initial_state.clone(),
            )
            .await
            .map_err(|e| match e {
                CompilerError::ServiceUnavailable(msg) => {
                    ApiError::ServiceUnavailable(format!(
                        "Compiler unavailable: {}",
                        msg
                    ))
                }
                other => ApiError::InternalServerError(format!(
                    "Compiler error: {}",
                    other
                )),
            })?;

        Ok(response)
    }

    /// Build a script: compile + store artifact + store source. No package/deployment.
    pub async fn build_script(
        &self,
        req: &BuildScriptRequest,
    ) -> BuildScriptResponse {
        // 1. Compile
        let wasm_bytes =
            match self.compiler.compile(&req.source, &req.language).await {
                Ok(bytes) => bytes,
                Err(CompilerError::CompilationFailed(errors)) => {
                    return BuildScriptResponse {
                        success: false,
                        artifact_id: None,
                        artifact_url: None,
                        wasm_size: None,
                        source_stored: false,
                        errors,
                        message: Some("Compilation failed".to_string()),
                    };
                }
                Err(e) => {
                    return BuildScriptResponse {
                        success: false,
                        artifact_id: None,
                        artifact_url: None,
                        wasm_size: None,
                        source_stored: false,
                        errors: vec![format!("Compiler error: {}", e)],
                        message: Some(
                            "Failed to reach compiler service".to_string(),
                        ),
                    };
                }
            };

        let wasm_size = wasm_bytes.len();

        // 2. Store artifact
        let artifact_id = match self.artifact_store.store(&wasm_bytes).await {
            Ok(id) => id,
            Err(e) => {
                error!(error = %e, "Failed to store artifact");
                return BuildScriptResponse {
                    success: false,
                    artifact_id: None,
                    artifact_url: None,
                    wasm_size: Some(wasm_size),
                    source_stored: false,
                    errors: vec![format!("Failed to store artifact: {}", e)],
                    message: None,
                };
            }
        };

        let artifact_url = format!(
            "{}/{}",
            self.artifact_base_url.trim_end_matches('/'),
            artifact_id
        );

        // 3. Store build metadata (non-fatal)
        let meta = ArtifactMeta {
            package: req.package_name.clone(),
            class_key: req.class_key.clone(),
            built_at: chrono::Utc::now().to_rfc3339(),
        };
        if let Err(e) =
            self.artifact_store.store_meta(&artifact_id, &meta).await
        {
            error!(error = %e, "Failed to store artifact metadata (non-fatal)");
        }

        // 4. Store source code (non-fatal)
        let source_stored = match self
            .source_store
            .store_source(&req.package_name, &req.class_key, &req.source)
            .await
        {
            Ok(()) => true,
            Err(e) => {
                error!(error = %e, "Failed to store source code (non-fatal)");
                false
            }
        };

        info!(
            artifact_id = %artifact_id,
            wasm_size = wasm_size,
            source_stored = source_stored,
            "Script built successfully"
        );

        BuildScriptResponse {
            success: true,
            artifact_id: Some(artifact_id),
            artifact_url: Some(artifact_url),
            wasm_size: Some(wasm_size),
            source_stored,
            errors: vec![],
            message: Some("Script built successfully".to_string()),
        }
    }

    /// Full compile → store → deploy pipeline.
    pub async fn deploy_script(
        &self,
        req: &DeployScriptRequest,
    ) -> DeployScriptResponse {
        // 1. Compile
        let wasm_bytes =
            match self.compiler.compile(&req.source, &req.language).await {
                Ok(bytes) => bytes,
                Err(CompilerError::CompilationFailed(errors)) => {
                    return DeployScriptResponse {
                        success: false,
                        deployment_key: None,
                        artifact_id: None,
                        artifact_url: None,
                        errors,
                        message: Some("Compilation failed".to_string()),
                    };
                }
                Err(e) => {
                    return DeployScriptResponse {
                        success: false,
                        deployment_key: None,
                        artifact_id: None,
                        artifact_url: None,
                        errors: vec![format!("Compiler error: {}", e)],
                        message: Some(
                            "Failed to reach compiler service".to_string(),
                        ),
                    };
                }
            };

        // 2. Store artifact
        let artifact_id = match self.artifact_store.store(&wasm_bytes).await {
            Ok(id) => id,
            Err(e) => {
                error!(error = %e, "Failed to store artifact");
                return DeployScriptResponse {
                    success: false,
                    deployment_key: None,
                    artifact_id: None,
                    artifact_url: None,
                    errors: vec![format!("Failed to store artifact: {}", e)],
                    message: None,
                };
            }
        };

        let artifact_url = format!(
            "{}/{}",
            self.artifact_base_url.trim_end_matches('/'),
            artifact_id
        );

        // 3. Store source code
        if let Err(e) = self
            .source_store
            .store_source(&req.package_name, &req.class_key, &req.source)
            .await
        {
            error!(error = %e, "Failed to store source code (non-fatal)");
        }

        // 4. Create/update package with WASM function
        let wasm_fn_key =
            format!("{}-{}-wasm", req.package_name, req.class_key);

        let function = OFunction {
            key: wasm_fn_key.clone(),
            function_type: FunctionType::Wasm,
            description: Some(format!(
                "Auto-generated WASM function for {}/{}",
                req.package_name, req.class_key
            )),
            provision_config: Some(ProvisionConfig {
                wasm_module_url: Some(artifact_url.clone()),
                ..Default::default()
            }),
            config: Default::default(),
        };

        // Build function bindings for the class
        let bindings: Vec<FunctionBinding> = if req.function_bindings.is_empty()
        {
            // Default: single binding mapping the class key to the wasm function
            vec![FunctionBinding {
                name: req.class_key.clone(),
                function_key: wasm_fn_key.clone(),
                access_modifier: FunctionAccessModifier::Public,
                stateless: false,
                parameters: vec![],
            }]
        } else {
            req.function_bindings
                .iter()
                .map(|b| FunctionBinding {
                    name: b.name.clone(),
                    function_key: wasm_fn_key.clone(),
                    access_modifier: FunctionAccessModifier::Public,
                    stateless: b.stateless,
                    parameters: vec![],
                })
                .collect()
        };

        let class = OClass {
            key: req.class_key.clone(),
            description: Some(format!("Script class for {}", req.class_key)),
            state_spec: None,
            function_bindings: bindings,
        };

        // Try to update existing package or create new one
        let package = OPackage {
            name: req.package_name.clone(),
            version: Some("1.0.0".to_string()),
            metadata: Default::default(),
            classes: vec![class.clone()],
            functions: vec![function],
            dependencies: vec![],
            deployments: vec![],
        };

        if let Err(e) = self.ensure_package(&package).await {
            error!(error = %e, "Failed to create/update package");
            return DeployScriptResponse {
                success: false,
                deployment_key: None,
                artifact_id: Some(artifact_id),
                artifact_url: Some(artifact_url),
                errors: vec![format!("Failed to create package: {}", e)],
                message: None,
            };
        }

        // 5. Create deployment
        let deployment_key = format!("{}-{}", req.package_name, req.class_key);
        let deployment = OClassDeployment {
            key: deployment_key.clone(),
            package_name: req.package_name.clone(),
            class_key: req.class_key.clone(),
            target_envs: req.target_envs.clone(),
            condition: DeploymentCondition::Pending,
            ..Default::default()
        };

        match self
            .deployment_service
            .deploy_class(&class, &deployment)
            .await
        {
            Ok(dep_id) => {
                info!(
                    deployment_key = %dep_id,
                    artifact_id = %artifact_id,
                    "Script deployed successfully"
                );
                DeployScriptResponse {
                    success: true,
                    deployment_key: Some(dep_id.to_string()),
                    artifact_id: Some(artifact_id),
                    artifact_url: Some(artifact_url),
                    errors: vec![],
                    message: Some("Script deployed successfully".to_string()),
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to deploy script");
                DeployScriptResponse {
                    success: false,
                    deployment_key: None,
                    artifact_id: Some(artifact_id),
                    artifact_url: Some(artifact_url),
                    errors: vec![format!("Deployment failed: {}", e)],
                    message: None,
                }
            }
        }
    }

    /// Get stored source code for a script.
    pub async fn get_script_source(
        &self,
        package: &str,
        function: &str,
    ) -> Result<Option<ScriptSourceResponse>, ArtifactError> {
        match self.source_store.get_source(package, function).await? {
            Some(source) => Ok(Some(ScriptSourceResponse {
                package: package.to_string(),
                function: function.to_string(),
                source,
                language: "typescript".to_string(),
            })),
            None => Ok(None),
        }
    }

    /// Ensure the package exists (create or update).
    async fn ensure_package(
        &self,
        package: &OPackage,
    ) -> Result<(), PackageManagerError> {
        match self.package_service.get_package(&package.name).await? {
            Some(_) => {
                info!(package = %package.name, "Updating existing package");
                self.package_service.update_package(package.clone()).await?;
            }
            None => {
                info!(package = %package.name, "Creating new package");
                self.package_service.create_package(package.clone()).await?;
            }
        }
        Ok(())
    }

    /// Check compiler health.
    pub async fn compiler_health(&self) -> Result<(), CompilerError> {
        self.compiler.health().await
    }

    /// List all stored artifacts with enriched metadata.
    pub async fn list_artifacts(
        &self,
    ) -> Result<Vec<ArtifactListEntry>, ArtifactError> {
        let artifacts = self.artifact_store.list().await?;

        let entries = artifacts
            .into_iter()
            .map(|a| {
                let url = format!(
                    "{}/{}",
                    self.artifact_base_url.trim_end_matches('/'),
                    a.id
                );
                let (source_package, source_function, built_at) = match &a.meta
                {
                    Some(m) => (
                        Some(m.package.clone()),
                        Some(m.class_key.clone()),
                        Some(m.built_at.clone()),
                    ),
                    None => (None, None, None),
                };
                ArtifactListEntry {
                    id: a.id,
                    size: a.size,
                    url,
                    has_source: source_package.is_some(),
                    source_package,
                    source_function,
                    built_at,
                }
            })
            .collect();

        Ok(entries)
    }

    /// List all stored source entries.
    pub async fn list_sources(
        &self,
    ) -> Result<Vec<crate::services::artifact::SourceInfo>, ArtifactError> {
        self.source_store.list_sources().await
    }
}

// Convert ArtifactError to ApiError
impl From<ArtifactError> for ApiError {
    fn from(e: ArtifactError) -> Self {
        match e {
            ArtifactError::NotFound(id) => {
                ApiError::NotFound(format!("Artifact not found: {}", id))
            }
            ArtifactError::Io(e) => {
                ApiError::InternalServerError(format!("IO error: {}", e))
            }
            ArtifactError::InvalidId(id) => {
                ApiError::BadRequest(format!("Invalid artifact ID: {}", id))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_language() {
        assert_eq!(default_language(), "typescript");
    }

    #[test]
    fn test_compile_request_deserialize() {
        let json = r#"{"source": "const x = 1;", "language": "typescript"}"#;
        let req: CompileRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.source, "const x = 1;");
        assert_eq!(req.language, "typescript");
    }

    #[test]
    fn test_compile_request_default_language() {
        let json = r#"{"source": "const x = 1;"}"#;
        let req: CompileRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.language, "typescript");
    }

    #[test]
    fn test_deploy_request_deserialize() {
        let json = r#"{
            "source": "class Counter {}",
            "package_name": "test-pkg",
            "class_key": "Counter",
            "function_bindings": [
                {"name": "increment", "stateless": false},
                {"name": "reset", "stateless": true}
            ],
            "target_envs": ["edge-1"]
        }"#;
        let req: DeployScriptRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.package_name, "test-pkg");
        assert_eq!(req.class_key, "Counter");
        assert_eq!(req.function_bindings.len(), 2);
        assert!(!req.function_bindings[0].stateless);
        assert!(req.function_bindings[1].stateless);
        assert_eq!(req.target_envs, vec!["edge-1"]);
    }

    #[test]
    fn test_deploy_request_defaults() {
        let json = r#"{
            "source": "class X {}",
            "package_name": "pkg",
            "class_key": "X"
        }"#;
        let req: DeployScriptRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.language, "typescript");
        assert!(req.function_bindings.is_empty());
        assert!(req.target_envs.is_empty());
    }

    #[test]
    fn test_compile_response_serialize_success() {
        let resp = CompileResponse {
            success: true,
            errors: vec![],
            wasm_size: Some(1024),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"wasm_size\":1024"));
        assert!(!json.contains("errors"));
    }

    #[test]
    fn test_compile_response_serialize_failure() {
        let resp = CompileResponse {
            success: false,
            errors: vec!["Type error".to_string()],
            wasm_size: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("Type error"));
        assert!(!json.contains("wasm_size"));
    }

    #[test]
    fn test_deploy_response_serialize() {
        let resp = DeployScriptResponse {
            success: true,
            deployment_key: Some("dep-123".to_string()),
            artifact_id: Some("abc123".to_string()),
            artifact_url: Some("http://pm/artifacts/abc123".to_string()),
            errors: vec![],
            message: Some("Deployed".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("dep-123"));
        assert!(json.contains("abc123"));
        assert!(!json.contains("errors"));
    }

    #[test]
    fn test_artifact_error_to_api_error_not_found() {
        let err = ApiError::from(ArtifactError::NotFound("abc".to_string()));
        match err {
            ApiError::NotFound(msg) => assert!(msg.contains("abc")),
            _ => panic!("Wrong error variant"),
        }
    }

    #[test]
    fn test_artifact_error_to_api_error_invalid_id() {
        let err = ApiError::from(ArtifactError::InvalidId("bad".to_string()));
        match err {
            ApiError::BadRequest(msg) => assert!(msg.contains("bad")),
            _ => panic!("Wrong error variant"),
        }
    }
}
