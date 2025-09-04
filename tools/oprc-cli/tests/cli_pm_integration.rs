use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::{process::Command, time::Duration};
use tempfile::TempDir;
use tokio::time::sleep;

use oprc_pm as pm;

// Build and start an in-memory PM HTTP server on a random port
async fn start_pm_server() -> anyhow::Result<(TempDir, String)> {
    // Force in-memory and an ephemeral port
    unsafe {
        std::env::set_var("OPRC_PM_STORAGE_TYPE", "memory");
        std::env::set_var("OPRC_PM_SERVER_HOST", "127.0.0.1");
        std::env::set_var("OPRC_PM_SERVER_PORT", "0");
        std::env::set_var("OPRC_PM_CRM_DEFAULT_URL", "http://localhost:8081");
    }

    // Build the real ApiServer from env and serve it on an ephemeral port
    let server = pm::build_api_server_from_env().await?;
    // Bind manually to get the port and to run in background
    let listener =
        tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await?;
    let addr = listener.local_addr()?;
    let base_url = format!("http://{}:{}", addr.ip(), addr.port());

    let app = server.into_router();
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("PM server error: {e}");
        }
    });

    // Wait briefly for server to come up
    sleep(Duration::from_millis(200)).await;

    // Temp dir to hold CLI config
    let temp_dir = TempDir::new()?;

    Ok((temp_dir, base_url))
}

async fn seed_package(pm_base: &str) -> anyhow::Result<()> {
    // Minimal valid package
    let pkg = oprc_models::OPackage {
        name: "cli-int-test-pkg".to_string(),
        version: Some("1.0.0".to_string()),
        metadata: oprc_models::PackageMetadata {
            author: Some("cli-int".to_string()),
            description: Some("integration".to_string()),
            tags: vec![],
            created_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
        },
        classes: vec![oprc_models::OClass {
            key: "CliClass".to_string(),
            description: Some("For CLI integration test".to_string()),
            function_bindings: vec![oprc_models::FunctionBinding {
                name: "hello".to_string(),
                function_key: "hello".to_string(),
                access_modifier: pm::FunctionAccessModifier::Public,
                stateless: false,
                parameters: vec![],
            }],
            state_spec: None,
        }],
        functions: vec![oprc_models::OFunction {
            key: "hello".to_string(),
            function_type: pm::FunctionType::Custom,
            description: Some("say hello".to_string()),
            provision_config: None,
            config: std::collections::HashMap::new(),
        }],
        dependencies: vec![],
        deployments: vec![],
    };

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/packages", pm_base))
        .json(&pkg)
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "seed package failed: {}",
        resp.status()
    );
    Ok(())
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
                "gateway_url": format!("{}", pm_base),
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

// No custom health handler; we use PM server's built-in routes

#[tokio::test(flavor = "multi_thread")]
async fn cli_lists_classes_and_functions_against_pm() -> anyhow::Result<()> {
    let (tmp, pm_base) = start_pm_server().await?;
    seed_package(&pm_base).await?;
    let cfg_path = write_cli_config(&tmp, &pm_base).await?;

    // Ensure CLI reads this config
    let mut cmd = Command::cargo_bin("oprc-cli").expect("binary built");
    cmd.env("OPRC_CONFIG_PATH", &cfg_path);

    // 1) List classes
    let assert = cmd.arg("class").arg("list").assert();
    assert
        .success()
        .stdout(predicate::str::contains("CliClass"));

    // 2) List functions
    let mut cmd2 = Command::cargo_bin("oprc-cli").expect("binary built");
    cmd2.env("OPRC_CONFIG_PATH", &cfg_path);
    let assert2 = cmd2.arg("function").arg("list").assert();
    assert2.success().stdout(predicate::str::contains("hello"));

    // 3) Deploy list (should be empty array)
    let mut cmd3 = Command::cargo_bin("oprc-cli").expect("binary built");
    cmd3.env("OPRC_CONFIG_PATH", &cfg_path);
    let assert3 = cmd3.arg("deploy").arg("list").assert();
    assert3.success().stdout(predicate::str::contains("["));

    // 4) Package delete via CLI using an OPackage-shaped YAML containing the seeded package name
    let yaml_content = r#"name: cli-int-test-pkg
version: "1.0.0"
disabled: false
metadata:
    author: null
    description: null
    tags: []
classes: []
functions: []
dependencies: []
deployments: []
"#;
    let yaml_path = tmp.path().join("delete-pkg.yaml");
    tokio::fs::write(&yaml_path, yaml_content).await?;

    let mut cmd4 = Command::cargo_bin("oprc-cli").expect("binary built");
    cmd4.env("OPRC_CONFIG_PATH", &cfg_path);
    let assert4 = cmd4
        .arg("package")
        .arg("delete")
        .arg(yaml_path.as_os_str())
        .assert();
    assert4
        .success()
        .stdout(predicate::str::contains("deleted successfully"));

    // 5) Classes list again should not contain CliClass
    let mut cmd5 = Command::cargo_bin("oprc-cli").expect("binary built");
    cmd5.env("OPRC_CONFIG_PATH", &cfg_path);
    let assert5 = cmd5.arg("class").arg("list").assert();
    assert5
        .success()
        .stdout(predicate::str::contains("CliClass").not());

    Ok(())
}
