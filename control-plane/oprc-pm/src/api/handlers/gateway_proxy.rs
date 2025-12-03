//! Generic Gateway reverse proxy handler.
//!
//! Forwards all requests under `/api/gateway/*` to the Gateway service,
//! similar to nginx reverse proxy. This allows the PM to act as a single
//! entry point for the GUI without defining specific routes for each Gateway API.

use crate::{errors::ApiError, server::AppState};
use axum::{
    body::Body,
    extract::{Request, State},
    http::{Method, StatusCode},
    response::Response,
};
use reqwest::Client;
use std::error::Error as StdError;
use std::time::Duration;
use tracing::{debug, error};

/// Configuration for the Gateway proxy client.
#[derive(Clone)]
pub struct GatewayProxy {
    client: Client,
    base_url: String,
}

impl GatewayProxy {
    pub fn new(base_url: String, timeout_seconds: u64) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .connect_timeout(Duration::from_secs(10))
            // TCP optimizations
            .tcp_nodelay(true)
            .tcp_keepalive(Duration::from_secs(60))
            // Allow reasonable connection pool for parallel requests
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(10)
            .build()
            .expect("Failed to create HTTP client");

        // Remove trailing slash from base_url
        let base_url = base_url.trim_end_matches('/').to_string();

        Self { client, base_url }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

/// Reverse proxy handler that forwards all requests to the Gateway.
///
/// Maps `/api/gateway/{path}` -> `{GATEWAY_URL}/{path}`
///
/// Example:
/// - `/api/gateway/api/class/pkg.cls/0/objects` -> `http://gateway:8080/api/class/pkg.cls/0/objects`
#[tracing::instrument(skip(state, request), fields(method = %request.method(), uri = %request.uri()))]
pub async fn gateway_proxy(
    State(state): State<AppState>,
    request: Request,
) -> Result<Response, ApiError> {
    let proxy = state.gateway_proxy.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable(
            "Gateway proxy not configured. Set GATEWAY_URL environment variable."
                .to_string(),
        )
    })?;

    // Extract the path after /api/gateway/
    let path = request.uri().path();
    let target_path = path
        .strip_prefix("/api/gateway/")
        .or_else(|| path.strip_prefix("/api/gateway"))
        .unwrap_or("");

    // Build the target URL
    let query = request
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();
    let target_url = format!("{}/{}{}", proxy.base_url, target_path, query);

    debug!("Proxying request to: {}", target_url);

    // Forward the request
    let method = request.method().clone();
    let headers = request.headers().clone();
    let body_bytes = axum::body::to_bytes(request.into_body(), usize::MAX)
        .await
        .map_err(|e| {
            error!("Failed to read request body: {}", e);
            ApiError::InternalServerError(format!("Failed to read request body: {}", e))
        })?;

    // Build the proxied request
    let mut req_builder = match method {
        Method::GET => proxy.client.get(&target_url),
        Method::POST => proxy.client.post(&target_url),
        Method::PUT => proxy.client.put(&target_url),
        Method::DELETE => proxy.client.delete(&target_url),
        Method::PATCH => proxy.client.patch(&target_url),
        Method::HEAD => proxy.client.head(&target_url),
        _ => {
            return Err(ApiError::BadRequest(format!(
                "Unsupported method: {}",
                method
            )));
        }
    };

    // Copy relevant headers (skip hop-by-hop headers)
    for (name, value) in headers.iter() {
        let name_str = name.as_str().to_lowercase();
        // Skip hop-by-hop headers
        if matches!(
            name_str.as_str(),
            "host" | "connection" | "keep-alive" | "transfer-encoding" | "te" | "trailer"
        ) {
            continue;
        }
        if let Ok(v) = value.to_str() {
            req_builder = req_builder.header(name.as_str(), v);
        }
    }

    // Add body for methods that support it
    if !body_bytes.is_empty() {
        req_builder = req_builder.body(body_bytes.to_vec());
    }

    // Send the request
    let response = req_builder.send().await.map_err(|e| {
        // Log detailed error information for debugging
        let is_timeout = e.is_timeout();
        let is_connect = e.is_connect();
        let is_request = e.is_request();
        error!(
            "Failed to proxy request to Gateway: {} (timeout={}, connect={}, request={}, source={:?})",
            e, is_timeout, is_connect, is_request, e.source()
        );
        ApiError::ServiceUnavailable(format!("Gateway request failed: {}", e))
    })?;

    // Convert reqwest response to axum response
    let status = StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let resp_headers = response.headers().clone();
    let body_bytes = response.bytes().await.map_err(|e| {
        error!("Failed to read Gateway response body: {}", e);
        ApiError::InternalServerError(format!("Failed to read Gateway response: {}", e))
    })?;

    // Build response
    let mut builder = Response::builder().status(status);

    // Copy response headers (skip hop-by-hop headers)
    for (name, value) in resp_headers.iter() {
        let name_str = name.as_str().to_lowercase();
        if matches!(
            name_str.as_str(),
            "connection" | "keep-alive" | "transfer-encoding" | "te" | "trailer"
        ) {
            continue;
        }
        builder = builder.header(name, value);
    }

    builder
        .body(Body::from(body_bytes.to_vec()))
        .map_err(|e| ApiError::InternalServerError(format!("Failed to build response: {}", e)))
}
