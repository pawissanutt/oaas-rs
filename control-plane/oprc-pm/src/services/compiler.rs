//! HTTP client for the compiler service (`oprc-compiler`).
//!
//! Sends TypeScript source code to the compiler and receives compiled WASM
//! bytes or error messages.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, info};

#[derive(Error, Debug)]
pub enum CompilerError {
    #[error("Compilation failed: {0:?}")]
    CompilationFailed(Vec<String>),

    #[error("Compiler service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Compiler request failed: {0}")]
    RequestFailed(String),

    #[error("Unexpected response from compiler: {0}")]
    UnexpectedResponse(String),
}

/// Response from failed compilation (JSON body).
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CompileErrorResponse {
    success: bool,
    #[serde(default)]
    errors: Vec<String>,
}

/// Request body sent to the compiler.
#[derive(Debug, Serialize)]
struct CompileRequest {
    source: String,
    language: String,
}

/// Request body sent to the compiler /test endpoint.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestScriptCompilerRequest {
    source: String,
    method_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_state: Option<serde_json::Value>,
}

/// Response from the compiler /test endpoint.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestScriptResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_state: Option<serde_json::Value>,
    #[serde(default)]
    pub logs: Vec<TestLogEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub duration_ms: f64,
}

/// A single log entry from script execution.
#[derive(Debug, Deserialize, Serialize)]
pub struct TestLogEntry {
    pub level: String,
    pub message: String,
}

/// Configuration for the compiler client.
#[derive(Debug, Clone)]
pub struct CompilerConfig {
    /// Base URL of the compiler service (e.g., `http://oprc-compiler:3000`).
    pub url: String,
    /// Request timeout in seconds.
    pub timeout_seconds: u64,
    /// Maximum number of retry attempts on transient failures.
    pub max_retries: u32,
}

impl Default for CompilerConfig {
    fn default() -> Self {
        Self {
            url: "http://oprc-compiler:3000".to_string(),
            timeout_seconds: 120,
            max_retries: 2,
        }
    }
}

/// Client for the OaaS compiler service.
pub struct CompilerClient {
    client: Client,
    config: CompilerConfig,
}

impl CompilerClient {
    /// Create a new compiler client with the given configuration.
    pub fn new(config: CompilerConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .expect("Failed to build reqwest client");
        Self { client, config }
    }

    /// Compile TypeScript source to a WASM Component.
    ///
    /// Returns the compiled WASM bytes on success.
    /// Returns `CompilerError::CompilationFailed` with error messages on compile errors.
    pub async fn compile(
        &self,
        source: &str,
        language: &str,
    ) -> Result<Vec<u8>, CompilerError> {
        let url = format!("{}/compile", self.config.url);
        let body = CompileRequest {
            source: source.to_string(),
            language: language.to_string(),
        };

        let mut last_err = None;
        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                debug!(attempt = attempt, "Retrying compile request");
                tokio::time::sleep(Duration::from_millis(500 * attempt as u64))
                    .await;
            }

            match self.do_compile(&url, &body).await {
                Ok(bytes) => return Ok(bytes),
                Err(CompilerError::ServiceUnavailable(msg)) => {
                    last_err = Some(CompilerError::ServiceUnavailable(msg));
                    continue; // retry
                }
                Err(e) => return Err(e), // non-retryable
            }
        }

        Err(last_err.unwrap_or_else(|| {
            CompilerError::ServiceUnavailable("exhausted retries".to_string())
        }))
    }

    async fn do_compile(
        &self,
        url: &str,
        body: &CompileRequest,
    ) -> Result<Vec<u8>, CompilerError> {
        let response =
            self.client.post(url).json(body).send().await.map_err(|e| {
                CompilerError::ServiceUnavailable(format!(
                    "Failed to connect to compiler: {}",
                    e
                ))
            })?;

        let status = response.status();

        if status.is_success() {
            // Check content type to determine if it's WASM or error JSON
            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();

            if content_type.contains("application/wasm") {
                let bytes = response.bytes().await.map_err(|e| {
                    CompilerError::RequestFailed(format!(
                        "Failed to read response body: {}",
                        e
                    ))
                })?;
                info!(size = bytes.len(), "Compilation successful");
                Ok(bytes.to_vec())
            } else {
                // Might be JSON with success: true but no wasm
                let text = response.text().await.map_err(|e| {
                    CompilerError::RequestFailed(format!(
                        "Failed to read response: {}",
                        e
                    ))
                })?;
                Err(CompilerError::UnexpectedResponse(format!(
                    "Expected application/wasm, got {}: {}",
                    content_type, text
                )))
            }
        } else if status.as_u16() == 400 {
            // Compiler returns 400 with JSON error body on compile errors
            let error_body: CompileErrorResponse =
                response.json().await.map_err(|e| {
                    CompilerError::UnexpectedResponse(format!(
                        "Failed to parse error response: {}",
                        e
                    ))
                })?;
            Err(CompilerError::CompilationFailed(error_body.errors))
        } else if status.is_server_error() {
            let text = response.text().await.unwrap_or_default();
            Err(CompilerError::ServiceUnavailable(format!(
                "Compiler returned {}: {}",
                status, text
            )))
        } else {
            let text = response.text().await.unwrap_or_default();
            Err(CompilerError::RequestFailed(format!(
                "Compiler returned {}: {}",
                status, text
            )))
        }
    }

    /// Check if the compiler service is healthy.
    pub async fn health(&self) -> Result<(), CompilerError> {
        let url = format!("{}/health", self.config.url);
        let response = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| {
                CompilerError::ServiceUnavailable(format!(
                    "Compiler health check failed: {}",
                    e
                ))
            })?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(CompilerError::ServiceUnavailable(format!(
                "Compiler unhealthy: status {}",
                response.status()
            )))
        }
    }

    /// Test a script method using the compiler service's real SDK runtime.
    ///
    /// Sends the source code to the compiler's `/test` endpoint, which
    /// bundles the code with the real `@oaas/sdk` and executes it in Node.js
    /// with mock infrastructure, ensuring behavior-consistent results.
    pub async fn test_script(
        &self,
        source: &str,
        method_name: &str,
        payload: Option<serde_json::Value>,
        initial_state: Option<serde_json::Value>,
    ) -> Result<TestScriptResponse, CompilerError> {
        let url = format!("{}/test", self.config.url);
        let body = TestScriptCompilerRequest {
            source: source.to_string(),
            method_name: method_name.to_string(),
            payload,
            initial_state,
        };

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                CompilerError::ServiceUnavailable(format!(
                    "Failed to connect to compiler for test: {}",
                    e
                ))
            })?;

        let status = response.status();
        if status.is_success() || status.as_u16() == 400 {
            let result: TestScriptResponse =
                response.json().await.map_err(|e| {
                    CompilerError::UnexpectedResponse(format!(
                        "Failed to parse test response: {}",
                        e
                    ))
                })?;
            Ok(result)
        } else {
            let text = response.text().await.unwrap_or_default();
            Err(CompilerError::RequestFailed(format!(
                "Compiler test returned {}: {}",
                status, text
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compiler_config_default() {
        let config = CompilerConfig::default();
        assert_eq!(config.url, "http://oprc-compiler:3000");
        assert_eq!(config.timeout_seconds, 120);
        assert_eq!(config.max_retries, 2);
    }

    #[test]
    fn test_compile_request_serialization() {
        let req = CompileRequest {
            source: "const x = 1;".to_string(),
            language: "typescript".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("source"));
        assert!(json.contains("language"));
        assert!(json.contains("typescript"));
    }

    #[test]
    fn test_compile_error_response_deserialization() {
        let json = r#"{"success": false, "errors": ["Type error at line 5"]}"#;
        let resp: CompileErrorResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.success);
        assert_eq!(resp.errors.len(), 1);
        assert!(resp.errors[0].contains("Type error"));
    }

    #[test]
    fn test_compile_error_response_empty_errors() {
        let json = r#"{"success": false}"#;
        let resp: CompileErrorResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.success);
        assert!(resp.errors.is_empty());
    }
}
