mod cli;
mod config;
mod infra;

use anyhow::{Context, Result};
use cli::OaasCli;
use config::TestConfig;
use envconfig::Envconfig;
use infra::InfraManager;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{error, info};

/// Tracks individual test case results for the final summary.
struct TestSummary {
    results: Vec<TestResult>,
    start: Instant,
}

struct TestResult {
    name: String,
    status: TestStatus,
    duration: Duration,
    detail: Option<String>,
}

enum TestStatus {
    Passed,
    Failed,
    Skipped,
}

impl TestSummary {
    fn new() -> Self {
        Self {
            results: Vec::new(),
            start: Instant::now(),
        }
    }

    fn record(&mut self, name: impl Into<String>, status: TestStatus, duration: Duration, detail: Option<String>) {
        self.results.push(TestResult {
            name: name.into(),
            status,
            duration,
            detail,
        });
    }

    fn print(&self) {
        let total = self.start.elapsed();
        let passed = self.results.iter().filter(|r| matches!(r.status, TestStatus::Passed)).count();
        let failed = self.results.iter().filter(|r| matches!(r.status, TestStatus::Failed)).count();
        let skipped = self.results.iter().filter(|r| matches!(r.status, TestStatus::Skipped)).count();

        println!();
        println!("══════════════════════════════════════════════════");
        println!("  E2E Test Summary");
        println!("══════════════════════════════════════════════════");
        for r in &self.results {
            let icon = match r.status {
                TestStatus::Passed  => "✅",
                TestStatus::Failed  => "❌",
                TestStatus::Skipped => "⏭️ ",
            };
            let detail = r.detail.as_deref().map(|d| format!(" ({})", d)).unwrap_or_default();
            println!("  {} {:.<40} {:>6.1}s{}", icon, r.name, r.duration.as_secs_f64(), detail);
        }
        println!("──────────────────────────────────────────────────");
        println!("  Total: {} passed, {} failed, {} skipped  ({:.1}s)",
            passed, failed, skipped, total.as_secs_f64());
        println!("══════════════════════════════════════════════════");
        println!();
    }

    fn has_failures(&self) -> bool {
        self.results.iter().any(|r| matches!(r.status, TestStatus::Failed))
    }
}

struct CleanupGuard {
    cli: OaasCli,
    package_path: String,
    no_cleanup: bool,
    /// Only attempt cleanup when the package was actually applied.
    armed: bool,
}

impl CleanupGuard {
    /// Mark the guard as armed — cleanup will run on drop.
    fn arm(&mut self) {
        self.armed = true;
    }
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        if !self.no_cleanup && self.armed {
            info!("Cleaning up package '{}'...", self.package_path);
            if let Err(e) = self.cli.delete_package(&self.package_path) {
                error!("Failed to cleanup package: {}", e);
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    info!("Starting System E2E Test...");

    let config = TestConfig::init_from_env()?;
    let infra = InfraManager::new(config.clone());
    let cli = OaasCli::new(config.clone());
    let mut summary = TestSummary::new();

    // Register cleanup hook (not armed until package is actually applied)
    let mut cleanup_guard = CleanupGuard {
        cli: OaasCli::new(config.clone()),
        package_path: "tests/system_e2e/fixtures/package.yaml".to_string(),
        no_cleanup: config.no_cleanup,
        armed: false,
    };

    // 1. Setup Infrastructure
    if config.skip_infra {
        info!("Skipping all infrastructure setup (OAAS_E2E_SKIP_INFRA=true)");
    } else {
        infra.check_requirements()?;
        infra.setup_kind()?;

        if config.skip_build {
            info!("Skipping image build (OAAS_E2E_SKIP_BUILD=true)");
        } else {
            infra.build_images()?;
        }

        if config.skip_load {
            info!("Skipping image load (OAAS_E2E_SKIP_LOAD=true)");
        } else {
            infra.load_images()?;
        }

        if config.skip_deploy {
            info!("Skipping deploy (OAAS_E2E_SKIP_DEPLOY=true)");
        } else {
            infra.deploy_system()?;
        }
    }

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

    // Resolve template variables in fixture files
    let registry = "localhost:5001";
    let resolved_pkg = resolve_fixture(
        "tests/system_e2e/fixtures/package.yaml",
        registry,
        &config.image_tag,
    )?;
    info!("Resolved package fixture: {}", resolved_pkg);

    // Apply Package
    let t = Instant::now();
    cli.apply_package(&resolved_pkg)?;
    cleanup_guard.arm();
    cli.apply_deployment("tests/system_e2e/fixtures/deployment.yaml")?;
    summary.record("Package & Deployment apply", TestStatus::Passed, t.elapsed(), None);

    // Wait for ODGM to be ready
    info!("Waiting for ODGM pods...");
    sleep(Duration::from_secs(10)).await;

    // Invoke Function
    // Retry a few times as it might take time to start
    let t = Instant::now();
    let mut success = false;
    let mut attempts = 0;
    for i in 0..5 {
        attempts = i + 1;
        info!("Invocation attempt {}", attempts);
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
        summary.record("Echo function invocation", TestStatus::Failed, t.elapsed(), Some(format!("{} attempts", attempts)));
        error!("Test Failed: Function invocation did not succeed.");
        summary.print();
        return Err(anyhow::anyhow!(
            "Test Failed: Function invocation did not succeed."
        ));
    }
    summary.record("Echo function invocation", TestStatus::Passed, t.elapsed(), Some(format!("{} attempt(s)", attempts)));

    // 4. Test Object Data API
    info!("Testing Object Data API...");
    let t = Instant::now();
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
        summary.record("Object Data API (set/get)", TestStatus::Passed, t.elapsed(), None);
    } else {
        error!(
            "Object Data API test failed. Expected {}, got {}",
            value, retrieved
        );
        summary.record("Object Data API (set/get)", TestStatus::Failed, t.elapsed(), Some(format!("expected '{}', got '{}'", value, retrieved)));
        summary.print();
        return Err(anyhow::anyhow!("Object Data API test failed"));
    }

    // 5. Test Stateful Function (Random)
    info!("Testing Stateful Function (Random)...");
    let t = Instant::now();
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
                        info!(
                            "Random function returned expected number of entries"
                        );
                        summary.record("Stateful function (random)", TestStatus::Passed, t.elapsed(), Some(format!("{} entries", obj.len())));
                    } else {
                        error!(
                            "Random function returned unexpected number of entries: {}",
                            obj.len()
                        );
                        summary.record("Stateful function (random)", TestStatus::Failed, t.elapsed(), Some(format!("expected 3 entries, got {}", obj.len())));
                        summary.print();
                        return Err(anyhow::anyhow!("Random function failed"));
                    }
                } else {
                    error!("Random function output is not a JSON object");
                    summary.record("Stateful function (random)", TestStatus::Failed, t.elapsed(), Some("not a JSON object".into()));
                    summary.print();
                    return Err(anyhow::anyhow!("Random function failed"));
                }
            } else {
                error!("Random function output is not valid JSON");
                summary.record("Stateful function (random)", TestStatus::Failed, t.elapsed(), Some("invalid JSON".into()));
                summary.print();
                return Err(anyhow::anyhow!("Random function failed"));
            }
        }
        Err(e) => {
            error!("Random invocation failed: {}", e);
            summary.record("Stateful function (random)", TestStatus::Failed, t.elapsed(), Some(e.to_string()));
            summary.print();
            return Err(e);
        }
    }

    // 6. WASM Function E2E Test (skip if WASM binary not built)
    let wasm_binary = "target/wasm32-wasip2/release/wasm_guest_echo.wasm";
    if std::path::Path::new(wasm_binary).exists() {
        info!("Running WASM function E2E test...");
        let t = Instant::now();
        match run_wasm_e2e_test(&cli, &infra).await {
            Ok(()) => {
                summary.record("WASM echo + transform", TestStatus::Passed, t.elapsed(), None);
            }
            Err(e) => {
                summary.record("WASM echo + transform", TestStatus::Failed, t.elapsed(), Some(e.to_string()));
                summary.print();
                return Err(e);
            }
        }
    } else {
        info!("Skipping WASM E2E test ({} not found)", wasm_binary);
        summary.record("WASM echo + transform", TestStatus::Skipped, Duration::ZERO, Some("wasm binary not built".into()));
    }

    summary.print();

    if summary.has_failures() {
        return Err(anyhow::anyhow!("Some E2E tests failed"));
    }

    info!("All E2E Tests Passed!");
    Ok(())
}

/// Run the WASM function E2E scenario.
///
/// Requires:
/// - The WASM guest echo module built: `cargo build -p wasm-guest-echo --target wasm32-wasip2 --release`
/// - ODGM deployed with the `wasm` feature enabled
///
/// Deploys an in-cluster nginx + ConfigMap file server so that ODGM pods can
/// download the `.wasm` binary.
async fn run_wasm_e2e_test(cli: &OaasCli, infra: &InfraManager) -> Result<()> {
    let wasm_binary = "target/wasm32-wasip2/release/wasm_guest_echo.wasm";

    // Deploy in-cluster WASM file server (ConfigMap + nginx)
    let base_url = infra.deploy_wasm_server(wasm_binary)?;
    let module_url = format!("{}/wasm_guest_echo.wasm", base_url);
    info!("WASM module URL (in-cluster): {}", module_url);

    // Write a temporary package YAML with the actual module URL substituted
    let pkg_template =
        std::fs::read_to_string("tests/system_e2e/fixtures/wasm-package.yaml")?;
    let pkg_resolved = pkg_template.replace("${WASM_MODULE_URL}", &module_url);
    let tmp_pkg = "tests/system_e2e/fixtures/wasm-package-resolved.yaml";
    std::fs::write(tmp_pkg, &pkg_resolved)?;
    info!("Resolved WASM package written to {}", tmp_pkg);

    // Apply WASM package
    cli.apply_package(tmp_pkg)?;

    // Apply WASM deployment
    cli.apply_deployment("tests/system_e2e/fixtures/wasm-deployment.yaml")?;

    // Wait for ODGM to create shards and load WASM modules
    info!("Waiting for WASM-enabled ODGM pods...");
    sleep(Duration::from_secs(15)).await;

    // Test 1: Stateless echo function
    info!("Testing WASM stateless echo function...");
    let mut wasm_echo_success = false;
    for i in 0..5 {
        info!("WASM echo attempt {}", i + 1);
        match cli.invoke("e2e-wasm-test.WasmClass", "0", "echo", "wasm-hello") {
            Ok(output) => {
                if output.contains("wasm-hello") {
                    info!("WASM echo successful! Output: {}", output);
                    wasm_echo_success = true;
                    break;
                } else {
                    error!("WASM echo returned unexpected output: {}", output);
                }
            }
            Err(e) => {
                error!("WASM echo invocation failed: {}", e);
            }
        }
        sleep(Duration::from_secs(5)).await;
    }

    if !wasm_echo_success {
        error!("WASM Test Failed: Echo invocation did not succeed.");
        return Err(anyhow::anyhow!(
            "WASM Test Failed: Echo invocation did not succeed."
        ));
    }

    // Test 2: Object method (transform)
    info!("Testing WASM object method (transform)...");
    let obj_id = "wasm-test-obj";

    // Seed the object with initial data
    cli.object_set(
        "e2e-wasm-test.WasmClass",
        "0",
        obj_id,
        "_raw",
        "hello-from-test",
    )?;
    sleep(Duration::from_secs(2)).await;

    // Invoke the transform function on the object
    match cli.invoke_obj(
        "e2e-wasm-test.WasmClass",
        "0",
        obj_id,
        "transform",
        "",
    ) {
        Ok(output) => {
            info!("WASM transform output: {}", output);
            // The guest appends " - seen by wasm"
            if output.contains("seen by wasm") {
                info!("WASM transform function passed!");
            } else {
                error!("WASM transform output missing expected suffix");
                return Err(anyhow::anyhow!("WASM transform test failed"));
            }
        }
        Err(e) => {
            error!("WASM transform invocation failed: {}", e);
            return Err(e);
        }
    }

    info!("WASM E2E Tests Passed!");
    Ok(())
}

/// Resolve template variables in a fixture YAML file.
///
/// Substitutes:
/// - `${REGISTRY}` → the local registry address (e.g. `localhost:5001`)
/// - `${IMAGE_TAG}` → the image tag (e.g. `dev`)
/// - `${WASM_MODULE_URL}` → kept as-is (resolved separately in WASM tests)
///
/// Returns the path to the resolved temporary file.
fn resolve_fixture(
    fixture_path: &str,
    registry: &str,
    image_tag: &str,
) -> Result<String> {
    let template = std::fs::read_to_string(fixture_path)
        .with_context(|| format!("Failed to read fixture: {}", fixture_path))?;

    let resolved = template
        .replace("${REGISTRY}", registry)
        .replace("${IMAGE_TAG}", image_tag);

    let resolved_path = fixture_path.replace(".yaml", "-resolved.yaml");
    std::fs::write(&resolved_path, &resolved).with_context(|| {
        format!("Failed to write resolved fixture: {}", resolved_path)
    })?;

    Ok(resolved_path)
}
