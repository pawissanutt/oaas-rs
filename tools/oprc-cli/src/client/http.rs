use super::error::ClientError;
use crate::config::ContextConfig;
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// HTTP client for interacting with OaaS APIs
pub struct HttpClient {
    client: Client,
    base_url: String,
}

impl HttpClient {
    /// Create a new HTTP client
    pub fn new(context: &ContextConfig) -> Result<Self, ClientError> {
        let base_url = context
            .pm_url
            .as_ref()
            .ok_or_else(|| ClientError::config_error("Package manager URL not configured"))?
            .clone();

        let client = Client::builder()
            .user_agent("oprc-cli/0.1.0")
            .build()
            .map_err(ClientError::RequestFailed)?;

        Ok(Self { client, base_url })
    }

    /// Make a GET request
    pub async fn get<T>(&self, path: &str) -> Result<T, ClientError>
    where
        T: for<'de> Deserialize<'de>,
    {
        let url = format!("{}{}", self.base_url, path);
        let response = self.client.get(&url).send().await.map_err(ClientError::RequestFailed)?;
        self.handle_response(response).await
    }

    /// Make a POST request with JSON body
    pub async fn post<B, T>(&self, path: &str, body: &B) -> Result<T, ClientError>
    where
        B: Serialize,
        T: for<'de> Deserialize<'de>,
    {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(ClientError::RequestFailed)?;
        self.handle_response(response).await
    }

    /// Make a DELETE request
    pub async fn delete<T>(&self, path: &str) -> Result<T, ClientError>
    where
        T: for<'de> Deserialize<'de>,
    {
        let url = format!("{}{}", self.base_url, path);
        let response = self.client.delete(&url).send().await.map_err(ClientError::RequestFailed)?;
        self.handle_response(response).await
    }

    /// Handle HTTP response and deserialize JSON
    async fn handle_response<T>(&self, response: Response) -> Result<T, ClientError>
    where
        T: for<'de> Deserialize<'de>,
    {
        let status = response.status();
        
        if status.is_success() {
            let text = response.text().await.map_err(ClientError::RequestFailed)?;
            serde_json::from_str(&text).map_err(ClientError::SerializationError)
        } else {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            Err(ClientError::api_error(status.as_u16(), error_text))
        }
    }
}

/// Generic API response wrapper
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub data: T,
    pub status: String,
}

/// Package definition structure for API calls
#[derive(Debug, Serialize, Deserialize)]
pub struct PackageDefinition {
    pub name: String,
    pub classes: Vec<ClassDefinition>,
    pub functions: Vec<FunctionDefinition>,
}

/// Class definition structure
#[derive(Debug, Serialize, Deserialize)]
pub struct ClassDefinition {
    pub name: String,
    pub qos: Option<QosConfig>,
}

/// Function definition structure
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub qos: Option<QosConfig>,
}

/// QoS configuration
#[derive(Debug, Serialize, Deserialize)]
pub struct QosConfig {
    pub throughput: Option<i32>,
}

/// Class information from API
#[derive(Debug, Serialize, Deserialize)]
pub struct ClassInfo {
    pub name: String,
    pub pkg: String,
    pub status: String,
    pub qos: Option<QosConfig>,
}

/// Function information from API
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionInfo {
    pub name: String,
    pub pkg: String,
    pub cls: String,
    pub status: HashMap<String, String>,
}

/// Deployment information from API
#[derive(Debug, Serialize, Deserialize)]
pub struct DeploymentInfo {
    pub name: String,
    pub status: String,
    pub replicas: i32,
    pub ready: i32,
}

/// Runtime information from API
#[derive(Debug, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub name: String,
    pub class: String,
    pub status: String,
    pub instances: i32,
    pub cpu: f64,
    pub memory: f64,
}
