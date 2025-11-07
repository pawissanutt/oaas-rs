use assert_cmd::prelude::*; // assertion traits for std::process::Command
use axum::{
    Json, Router,
    routing::{get, post},
};
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::{process::Command, time::Duration};
use tempfile::TempDir;
use tokio::time::sleep;

async fn start_mock_pm()
-> anyhow::Result<(TempDir, String, Arc<Mutex<Vec<Value>>>)> {
    // Capture received deployment payloads
    let received: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let app = Router::new()
        // Minimal package create endpoint so package apply can succeed
        .route(
            "/api/v1/packages",
            post(|Json(_pkg): Json<Value>| async move {
                Ok::<_, axum::http::StatusCode>(Json(serde_json::json!({
                    "message": "package created"
                })))
            }),
        )
        // Deployments endpoint to capture posts
        .route(
            "/api/v1/deployments",
            post(move |Json(body): Json<Value>| {
                let recv = received_clone.clone();
                async move {
                    if let Ok(mut guard) = recv.lock() {
                        guard.push(body);
                    }
                    Ok::<_, axum::http::StatusCode>(Json(serde_json::json!({
                        "id": "mock-dep-id",
                        "status": "created",
                        "message": "ok"
                    })))
                }
            }),
        );
    // Also respond 404 to GET /api/v1/deployments/{key} by default
    let app = app.route(
        "/api/v1/deployments/{key}",
        get(|| async move {
            Err::<Json<Value>, axum::http::StatusCode>(
                axum::http::StatusCode::NOT_FOUND,
            )
        }),
    );

    let listener =
        tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await?;
    let addr: SocketAddr = listener.local_addr()?;
    let base_url = format!("http://{}:{}", addr.ip(), addr.port());

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("mock PM server error: {e}");
        }
    });

    // Temp dir for CLI config + YAML
    let temp_dir = TempDir::new()?;
    Ok((temp_dir, base_url, received))
}

async fn write_cli_config(
    dir: &TempDir,
    pm_base: &str,
) -> anyhow::Result<std::path::PathBuf> {
    let cfg = serde_yaml::to_string(&serde_json::json!({
        "contexts": {
            "default": {
                // pm_url now expects raw host (CLI adds /api/v1)
                "pm_url": pm_base,
                "gateway_url": pm_base,
                "default_class": "CliClass",
                "zenoh_peer": serde_json::Value::Null,
            }
        },
        "current_context": "default"
    }))?;

    let path = dir.path().join("config.yml");
    tokio::fs::write(&path, cfg).await?;
    Ok(path)
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_deploy_apply_posts_each_deployment() -> anyhow::Result<()> {
    let (tmp, base, received) = start_mock_pm().await?;
    let cfg_path = write_cli_config(&tmp, &base).await?;

    // Minimal OPackage with one deployment; package_name empty to test -p override
    let yaml = r#"name: pkg-yaml
version: "1.0.0"
metadata:
  author: null
  description: null
  tags: []
classes: []
functions: []
dependencies: []
deployments:
  - key: dep-1
    package_name: ""
    class_key: Cls
    nfr_requirements: {}
    functions: []
    condition: PENDING
    created_at: "2025-01-01T00:00:00Z"
    updated_at: "2025-01-01T00:00:00Z"
"#;

    let yaml_path = tmp.path().join("deploy-apply.yaml");
    tokio::fs::write(&yaml_path, yaml).await?;

    // Run CLI: should POST one deployment, filling package_name from -p override
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("oprc-cli"));
    cmd.env("OPRC_CONFIG_PATH", &cfg_path)
        .arg("deploy")
        .arg("apply")
        .arg(yaml_path.as_os_str())
        .arg("-p")
        .arg("pkg-override");

    let assert = cmd.assert();
    assert.success();

    // Give the server a moment to process
    sleep(Duration::from_millis(50)).await;

    // Validate one POST was received and package_name was overridden
    let bodies = received.lock().unwrap().clone();
    assert_eq!(bodies.len(), 1);
    let pkg_name = bodies[0]
        .get("package_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(pkg_name, "pkg-override");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_package_apply_with_flag_also_posts_deployments()
-> anyhow::Result<()> {
    let (tmp, base, received) = start_mock_pm().await?;
    let cfg_path = write_cli_config(&tmp, &base).await?;

    // OPackage with two deployments and an explicit package name
    let yaml = r#"name: pkg-yaml
version: "1.0.0"
metadata: { author: null, description: null, tags: [] }
classes: []
functions: []
dependencies: []
deployments:
  - key: dep-a
    package_name: pkg-yaml
    class_key: ClsA
    nfr_requirements: {}
    functions: []
    condition: PENDING
    created_at: "2025-01-01T00:00:00Z"
    updated_at: "2025-01-01T00:00:00Z"
  - key: dep-b
    package_name: ""
    class_key: ClsB
    nfr_requirements: {}
    functions: []
    condition: PENDING
    created_at: "2025-01-01T00:00:00Z"
    updated_at: "2025-01-01T00:00:00Z"
"#;

    let yaml_path = tmp.path().join("pkg-with-deps.yaml");
    tokio::fs::write(&yaml_path, yaml).await?;

    // Run CLI: package apply with --apply-deployments and override for empty package_name
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("oprc-cli"));
    cmd.env("OPRC_CONFIG_PATH", &cfg_path)
        .arg("package")
        .arg("apply")
        .arg(yaml_path.as_os_str())
        .arg("--apply-deployments")
        .arg("-p")
        .arg("pkg-override");

    let assert = cmd.assert();
    assert.success();

    sleep(Duration::from_millis(50)).await;
    let bodies = received.lock().unwrap().clone();
    // Expect two POSTs to /deployments
    assert_eq!(bodies.len(), 2);
    // dep-a should keep pkg-yaml, dep-b should be overridden
    let pkg_names: Vec<String> = bodies
        .iter()
        .map(|b| {
            b.get("package_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        })
        .collect();
    assert!(pkg_names.contains(&"pkg-yaml".to_string()));
    assert!(pkg_names.contains(&"pkg-override".to_string()));

    Ok(())
}
