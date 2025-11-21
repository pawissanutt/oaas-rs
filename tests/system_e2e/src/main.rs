mod cli;
mod config;
mod infra;

use anyhow::Result;
use cli::OaasCli;
use config::TestConfig;
use envconfig::Envconfig;
use infra::InfraManager;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};

struct CleanupGuard {
    cli: OaasCli,
    package_path: String,
    no_cleanup: bool,
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        if !self.no_cleanup {
            if let Err(e) = self.cli.delete_package(&self.package_path) {
                error!("Failed to cleanup package: {}", e);
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info,system_e2e=debug");
    }
    tracing_subscriber::fmt::init();

    info!("Starting System E2E Test...");

    let config = TestConfig::init_from_env()?;
    let infra = InfraManager::new(config.clone());
    let cli = OaasCli::new(config.clone());

    // Register cleanup hook
    let _cleanup_guard = CleanupGuard {
        cli: OaasCli::new(config.clone()),
        package_path: "tests/system_e2e/fixtures/package.yaml".to_string(),
        no_cleanup: config.no_cleanup,
    };

    // 1. Setup Infrastructure
    infra.check_requirements()?;
    infra.setup_kind()?;
    infra.build_images()?;
    infra.load_images()?;
    infra.deploy_system()?;

    // 2. Port Forward Services
    // PM -> 8081, Gateway -> 8082
    // let _pf_pm = infra.port_forward("oaas", "svc/oaas-pm-oprc-pm", "8081:8080")?;
    // let _pf_gw = infra.port_forward("oaas-1", "svc/oaas-crm-1-oprc-crm-gateway", "8082:80")?;

    // Configure CLI to use these ports
    // Using NodePorts mapped in kind-with-registry.sh
    // PM: 30180, Gateway: 30081
    cli.set_context("http://localhost:30180", "http://localhost:30081")?;

    // 3. Run Test Scenario
    info!("Running test scenario...");

    // Apply Package
    cli.apply_package("tests/system_e2e/fixtures/package.yaml")?;

    // Apply Deployment
    cli.apply_deployment("tests/system_e2e/fixtures/deployment.yaml")?;

    // Wait for ODGM to be ready
    // We can check if pods with label oaas.class=E2EClass are running
    info!("Waiting for ODGM pods...");
    // Simple sleep for now, or use kubectl wait
    sleep(Duration::from_secs(10)).await;

    // Invoke Function
    // Retry a few times as it might take time to start
    let mut success = false;
    for i in 0..5 {
        info!("Invocation attempt {}", i + 1);
        match cli.invoke("e2e-test.E2EClass", "0", "echo", "hello-world") {
            Ok(output) => {
                if output.contains("hello-world") {
                    info!("Invocation successful! Output: {}", output);
                    success = true;
                    break;
                } else {
                    error!("Invocation returned unexpected output: {}", output);
                }
            }
            Err(e) => {
                error!("Invocation failed: {}", e);
            }
        }
        sleep(Duration::from_secs(5)).await;
    }

    if !success {
        error!("Test Failed: Function invocation did not succeed.");
        return Err(anyhow::anyhow!(
            "Test Failed: Function invocation did not succeed."
        ));
    }

    // 4. Test Object Data API
    info!("Testing Object Data API...");
    let obj_id = "test-obj-1";
    let key = "mykey";
    let value = "myvalue";

    cli.object_set("e2e-test.E2EClass", "0", obj_id, key, value)?;

    sleep(Duration::from_secs(2)).await;

    let all_obj = cli.object_get_all("e2e-test.E2EClass", "0", obj_id)?;
    info!("Object content: {}", all_obj);

    let retrieved = cli.object_get("e2e-test.E2EClass", "0", obj_id, key)?;
    if retrieved.contains(value) {
        info!("Object Data API test passed!");
    } else {
        error!(
            "Object Data API test failed. Expected {}, got {}",
            value, retrieved
        );
        return Err(anyhow::anyhow!("Object Data API test failed"));
    }

    // 5. Test Stateful Function (Random)
    info!("Testing Stateful Function (Random)...");
    let random_payload = r#"{
        "entry": 3,
        "key_size": 5,
        "value_size": 5,
        "resp_json": true
    }"#;

    match cli.invoke_obj(
        "e2e-test.E2EClass",
        "0",
        obj_id,
        "random",
        random_payload,
    ) {
        Ok(output) => {
            info!("Random invocation successful! Output: {}", output);
            // Verify output is JSON
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&output)
            {
                if let Some(obj) = json.as_object() {
                    if obj.len() == 3 {
                        info!("Random function returned expected number of entries");
                    } else {
                        error!("Random function returned unexpected number of entries: {}", obj.len());
                        return Err(anyhow::anyhow!("Random function failed"));
                    }
                } else {
                    error!("Random function output is not a JSON object");
                    return Err(anyhow::anyhow!("Random function failed"));
                }
            } else {
                error!("Random function output is not valid JSON");
                return Err(anyhow::anyhow!("Random function failed"));
            }
        }
        Err(e) => {
            error!("Random invocation failed: {}", e);
            return Err(e);
        }
    }

    info!("Test Passed!");

    // 4. Teardown
    // infra.teardown()?;

    Ok(())
}
