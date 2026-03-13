use axum::Json;
use axum::Router;
use axum::routing::get;
use serde::Serialize;
use std::sync::Arc;

use crate::config::DevServerConfig;

/// Minimal deployment descriptor returned by the stub PM API.
#[derive(Serialize, Clone)]
struct StubDeployment {
    name: String,
    cls: String,
    #[serde(rename = "fnList")]
    fn_list: Vec<StubFunction>,
}

#[derive(Serialize, Clone)]
struct StubFunction {
    name: String,
}

#[derive(Serialize, Clone)]
struct StubPackage {
    name: String,
    classes: Vec<StubClass>,
}

#[derive(Serialize, Clone)]
struct StubClass {
    name: String,
    functions: Vec<StubFunction>,
}

/// Build stub PM API routes so the oprc-next frontend can populate its UI.
pub fn build_stub_api(config: &DevServerConfig) -> Router {
    let deployments: Vec<StubDeployment> = config
        .collections
        .iter()
        .map(|c| StubDeployment {
            name: c.name.clone(),
            cls: c.name.clone(),
            fn_list: c
                .functions
                .iter()
                .map(|f| StubFunction { name: f.id.clone() })
                .collect(),
        })
        .collect();

    let packages: Vec<StubPackage> = config
        .collections
        .iter()
        .map(|c| StubPackage {
            name: c.name.clone(),
            classes: vec![StubClass {
                name: c.name.clone(),
                functions: c
                    .functions
                    .iter()
                    .map(|f| StubFunction { name: f.id.clone() })
                    .collect(),
            }],
        })
        .collect();

    let deployments = Arc::new(deployments);
    let packages = Arc::new(packages);

    Router::new()
        .route(
            "/api/v1/deployments",
            get({
                let d = deployments.clone();
                move || async move { Json(d.as_ref().clone()) }
            }),
        )
        .route(
            "/api/v1/packages",
            get({
                let p = packages.clone();
                move || async move { Json(p.as_ref().clone()) }
            }),
        )
        .route("/api/v1/envs", get(|| async { Json(Vec::<()>::new()) }))
}
