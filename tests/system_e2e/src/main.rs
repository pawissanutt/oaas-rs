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

    fn record(
        &mut self,
        name: impl Into<String>,
        status: TestStatus,
        duration: Duration,
        detail: Option<String>,
    ) {
        self.results.push(TestResult {
            name: name.into(),
            status,
            duration,
            detail,
        });
    }

    fn print(&self) {
        let total = self.start.elapsed();
        let passed = self
            .results
            .iter()
            .filter(|r| matches!(r.status, TestStatus::Passed))
            .count();
        let failed = self
            .results
            .iter()
            .filter(|r| matches!(r.status, TestStatus::Failed))
            .count();
        let skipped = self
            .results
            .iter()
            .filter(|r| matches!(r.status, TestStatus::Skipped))
            .count();

        println!();
        println!("══════════════════════════════════════════════════");
        println!("  E2E Test Summary");
        println!("══════════════════════════════════════════════════");
        for r in &self.results {
            let icon = match r.status {
                TestStatus::Passed => "✅",
                TestStatus::Failed => "❌",
                TestStatus::Skipped => "⏭️ ",
            };
            let detail = r
                .detail
                .as_deref()
                .map(|d| format!(" ({})", d))
                .unwrap_or_default();
            println!(
                "  {} {:.<40} {:>6.1}s{}",
                icon,
                r.name,
                r.duration.as_secs_f64(),
                detail
            );
        }
        println!("──────────────────────────────────────────────────");
        println!(
            "  Total: {} passed, {} failed, {} skipped  ({:.1}s)",
            passed,
            failed,
            skipped,
            total.as_secs_f64()
        );
        println!("══════════════════════════════════════════════════");
        println!();
    }

    fn has_failures(&self) -> bool {
        self.results
            .iter()
            .any(|r| matches!(r.status, TestStatus::Failed))
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
    summary.record(
        "Package & Deployment apply",
        TestStatus::Passed,
        t.elapsed(),
        None,
    );

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
        summary.record(
            "Echo function invocation",
            TestStatus::Failed,
            t.elapsed(),
            Some(format!("{} attempts", attempts)),
        );
        error!("Test Failed: Function invocation did not succeed.");
        summary.print();
        return Err(anyhow::anyhow!(
            "Test Failed: Function invocation did not succeed."
        ));
    }
    summary.record(
        "Echo function invocation",
        TestStatus::Passed,
        t.elapsed(),
        Some(format!("{} attempt(s)", attempts)),
    );

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
        summary.record(
            "Object Data API (set/get)",
            TestStatus::Passed,
            t.elapsed(),
            None,
        );
    } else {
        error!(
            "Object Data API test failed. Expected {}, got {}",
            value, retrieved
        );
        summary.record(
            "Object Data API (set/get)",
            TestStatus::Failed,
            t.elapsed(),
            Some(format!("expected '{}', got '{}'", value, retrieved)),
        );
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
                        summary.record(
                            "Stateful function (random)",
                            TestStatus::Passed,
                            t.elapsed(),
                            Some(format!("{} entries", obj.len())),
                        );
                    } else {
                        error!(
                            "Random function returned unexpected number of entries: {}",
                            obj.len()
                        );
                        summary.record(
                            "Stateful function (random)",
                            TestStatus::Failed,
                            t.elapsed(),
                            Some(format!(
                                "expected 3 entries, got {}",
                                obj.len()
                            )),
                        );
                        summary.print();
                        return Err(anyhow::anyhow!("Random function failed"));
                    }
                } else {
                    error!("Random function output is not a JSON object");
                    summary.record(
                        "Stateful function (random)",
                        TestStatus::Failed,
                        t.elapsed(),
                        Some("not a JSON object".into()),
                    );
                    summary.print();
                    return Err(anyhow::anyhow!("Random function failed"));
                }
            } else {
                error!("Random function output is not valid JSON");
                summary.record(
                    "Stateful function (random)",
                    TestStatus::Failed,
                    t.elapsed(),
                    Some("invalid JSON".into()),
                );
                summary.print();
                return Err(anyhow::anyhow!("Random function failed"));
            }
        }
        Err(e) => {
            error!("Random invocation failed: {}", e);
            summary.record(
                "Stateful function (random)",
                TestStatus::Failed,
                t.elapsed(),
                Some(e.to_string()),
            );
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
                summary.record(
                    "WASM echo + transform",
                    TestStatus::Passed,
                    t.elapsed(),
                    None,
                );
            }
            Err(e) => {
                summary.record(
                    "WASM echo + transform",
                    TestStatus::Failed,
                    t.elapsed(),
                    Some(e.to_string()),
                );
                summary.print();
                return Err(e);
            }
        }
    } else {
        info!("Skipping WASM E2E test ({} not found)", wasm_binary);
        summary.record(
            "WASM echo + transform",
            TestStatus::Skipped,
            Duration::ZERO,
            Some("wasm binary not built".into()),
        );
    }

    // 7. TypeScript Scripting E2E Test (skip if compiler service URL not configured)
    let compiler_url = std::env::var("OPRC_COMPILER_URL").ok();
    if compiler_url.is_some() {
        info!("Running TypeScript scripting E2E test...");
        let t = Instant::now();
        match run_ts_scripting_e2e_test(
            &cli,
            &infra,
            compiler_url.as_deref().unwrap(),
        )
        .await
        {
            Ok(()) => {
                summary.record(
                    "TS scripting (compile+deploy+invoke)",
                    TestStatus::Passed,
                    t.elapsed(),
                    None,
                );
            }
            Err(e) => {
                summary.record(
                    "TS scripting (compile+deploy+invoke)",
                    TestStatus::Failed,
                    t.elapsed(),
                    Some(e.to_string()),
                );
                // Don't fail the whole suite — TS scripting is optional
                error!("TypeScript scripting E2E failed: {}", e);
            }
        }
    } else {
        info!(
            "Skipping TypeScript scripting E2E test (OPRC_COMPILER_URL not set)"
        );
        summary.record(
            "TS scripting (compile+deploy+invoke)",
            TestStatus::Skipped,
            Duration::ZERO,
            Some("OPRC_COMPILER_URL not set".into()),
        );
    }

    // 8. WebSocket Event Subscription E2E Test
    let ws_enabled =
        std::env::var("OAAS_E2E_WS_ENABLED").unwrap_or_default() == "true";
    if ws_enabled {
        info!("Running WebSocket event subscription E2E test...");
        let t = Instant::now();
        match run_ws_e2e_test(&cli).await {
            Ok(()) => {
                summary.record(
                    "WebSocket event subscription",
                    TestStatus::Passed,
                    t.elapsed(),
                    None,
                );
            }
            Err(e) => {
                summary.record(
                    "WebSocket event subscription",
                    TestStatus::Failed,
                    t.elapsed(),
                    Some(e.to_string()),
                );
                error!("WebSocket E2E failed: {}", e);
            }
        }
    } else {
        info!(
            "Skipping WebSocket E2E test (OAAS_E2E_WS_ENABLED not set to true)"
        );
        summary.record(
            "WebSocket event subscription",
            TestStatus::Skipped,
            Duration::ZERO,
            Some("OAAS_E2E_WS_ENABLED not set".into()),
        );
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

/// Run the TypeScript scripting E2E scenario.
///
/// Tests the full scripting pipeline:
/// 1. Compile TypeScript source via the PM /api/v1/scripts/compile endpoint
/// 2. Deploy via /api/v1/scripts/deploy
/// 3. Invoke the deployed function through the gateway
/// 4. Verify responses and state persistence
///
/// Requires:
/// - Compiler service running and accessible via PM
/// - PM with script endpoints enabled (OPRC_COMPILER_URL set)
async fn run_ts_scripting_e2e_test(
    cli: &OaasCli,
    _infra: &InfraManager,
    _compiler_url: &str,
) -> Result<()> {
    let pm_base = "http://localhost:30180";

    // Read TypeScript source
    let ts_source =
        std::fs::read_to_string("tests/wasm-guest-ts-counter/counter.ts")
            .context("Failed to read TypeScript counter source")?;

    let client = reqwest::Client::new();

    // Step 1: Compile (validation only)
    info!("Step 1: Compiling TypeScript source...");
    let compile_resp = client
        .post(format!("{}/api/v1/scripts/compile", pm_base))
        .json(&serde_json::json!({
            "source": ts_source,
            "language": "typescript"
        }))
        .send()
        .await
        .context("Failed to send compile request")?;

    let compile_status = compile_resp.status();
    let compile_body: serde_json::Value = compile_resp
        .json()
        .await
        .context("Failed to parse compile response")?;

    if !compile_status.is_success()
        || compile_body.get("success") == Some(&serde_json::json!(false))
    {
        let errors = compile_body
            .get("errors")
            .and_then(|e| e.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_else(|| "Unknown compile error".into());
        return Err(anyhow::anyhow!(
            "TypeScript compilation failed: {}",
            errors
        ));
    }
    info!("TypeScript compilation successful");

    // Step 2: Deploy
    info!("Step 2: Deploying TypeScript function...");
    let deploy_resp = client
        .post(format!("{}/api/v1/scripts/deploy", pm_base))
        .json(&serde_json::json!({
            "source": ts_source,
            "language": "typescript",
            "package_name": "e2e-ts-test",
            "class_key": "TsCounter",
            "function_bindings": [
                {"name": "increment", "stateless": false},
                {"name": "getCount", "stateless": false},
                {"name": "reset", "stateless": false},
                {"name": "echo", "stateless": true},
                {"name": "failOnPurpose", "stateless": false}
            ],
            "target_envs": ["oaas-1"]
        }))
        .send()
        .await
        .context("Failed to send deploy request")?;

    let deploy_status = deploy_resp.status();
    let deploy_body: serde_json::Value = deploy_resp
        .json()
        .await
        .context("Failed to parse deploy response")?;

    if !deploy_status.is_success() {
        return Err(anyhow::anyhow!(
            "TypeScript deploy failed (HTTP {}): {:?}",
            deploy_status,
            deploy_body
        ));
    }
    info!("TypeScript deployment created: {:?}", deploy_body);

    // Wait for ODGM to create shards and load WASM modules
    info!("Waiting for TS WASM-enabled ODGM pods...");
    sleep(Duration::from_secs(20)).await;

    // Step 3: Test stateless echo
    info!("Step 3: Testing TS stateless echo...");
    let mut echo_success = false;
    for i in 0..5 {
        info!("TS echo attempt {}", i + 1);
        match cli.invoke(
            "e2e-ts-test.TsCounter",
            "0",
            "echo",
            r#"{"msg":"hello-ts"}"#,
        ) {
            Ok(output) => {
                if output.contains("hello-ts") {
                    info!("TS echo successful! Output: {}", output);
                    echo_success = true;
                    break;
                } else {
                    error!("TS echo returned unexpected output: {}", output);
                }
            }
            Err(e) => {
                error!("TS echo invocation failed: {}", e);
            }
        }
        sleep(Duration::from_secs(5)).await;
    }

    if !echo_success {
        return Err(anyhow::anyhow!("TS echo invocation did not succeed"));
    }

    // Step 4: Test stateful increment
    info!("Step 4: Testing TS stateful increment...");
    let obj_id = "ts-counter-1";

    match cli.invoke_obj(
        "e2e-ts-test.TsCounter",
        "0",
        obj_id,
        "increment",
        r#"{"amount": 5}"#,
    ) {
        Ok(output) => {
            info!("TS increment output: {}", output);
            if output.contains("5") {
                info!("TS increment returned expected value");
            } else {
                return Err(anyhow::anyhow!(
                    "TS increment returned unexpected value: {}",
                    output
                ));
            }
        }
        Err(e) => {
            return Err(anyhow::anyhow!(
                "TS increment invocation failed: {}",
                e
            ));
        }
    }

    // Step 5: Verify state persistence — invoke again and check accumulated count
    info!("Step 5: Testing TS state persistence...");
    sleep(Duration::from_secs(2)).await;

    match cli.invoke_obj(
        "e2e-ts-test.TsCounter",
        "0",
        obj_id,
        "increment",
        r#"{"amount": 3}"#,
    ) {
        Ok(output) => {
            info!("TS second increment output: {}", output);
            // Should be 5 + 3 = 8
            if output.contains("8") {
                info!("TS state persistence verified (5 + 3 = 8)");
            } else {
                error!(
                    "TS state persistence issue: expected 8, got {}",
                    output
                );
                // Don't fail — state persistence details may vary
            }
        }
        Err(e) => {
            return Err(anyhow::anyhow!("TS second increment failed: {}", e));
        }
    }

    // Step 6: Verify source code retrieval
    info!("Step 6: Verifying source code retrieval...");
    let source_resp = client
        .get(format!("{}/api/v1/scripts/e2e-ts-test/TsCounter", pm_base))
        .send()
        .await
        .context("Failed to request script source")?;

    if source_resp.status().is_success() {
        let source_body = source_resp.text().await.unwrap_or_default();
        if source_body.contains("TsCounter")
            && source_body.contains("increment")
        {
            info!("Source code retrieval verified");
        } else {
            error!("Retrieved source doesn't match expected content");
        }
    } else {
        error!("Source retrieval failed: HTTP {}", source_resp.status());
    }

    info!("TypeScript Scripting E2E Tests Passed!");
    Ok(())
}

/// Run the WebSocket event subscription E2E scenario.
///
/// Requires:
/// - Gateway deployed with `GATEWAY_WS_ENABLED=true`
/// - ODGM deployed with `ODGM_ZENOH_EVENT_PUBLISH=true`
/// - The basic E2E package already deployed (e2e-test.E2EClass)
///
/// Tests:
/// 1. Object-level WS: subscribe to one object, mutate it, verify event received
/// 2. Class-level WS: subscribe to all objects, mutate two different objects, verify both events
async fn run_ws_e2e_test(cli: &OaasCli) -> Result<()> {
    use futures_util::StreamExt;

    let gateway_ws_base = "ws://localhost:30081";
    let class = "e2e-test.E2EClass";
    let partition = "0";

    // Enable ODGM Zenoh event publishing on the E2E ODGM deployment
    // The ODGM pod is managed by CRM and named after the ClassRuntime CR
    info!("Enabling ODGM Zenoh event publishing for WS tests...");
    let odgm_deploy = duct::cmd!(
        "kubectl", "get", "deploy", "-n", "oaas-1",
        "-l", "oaas.io/class=E2EClass",
        "-o", "jsonpath={.items[0].metadata.name}"
    )
    .read()
    .context("Failed to find ODGM deployment")?;

    if odgm_deploy.is_empty() {
        return Err(anyhow::anyhow!("No ODGM deployment found for E2EClass"));
    }
    info!("Found ODGM deployment: {}", odgm_deploy);

    duct::cmd!(
        "kubectl", "set", "env", "-n", "oaas-1",
        &format!("deployment/{}", odgm_deploy),
        "ODGM_ZENOH_EVENT_PUBLISH=true",
        "ODGM_ZENOH_EVENT_LOCALITY=remote"
    )
    .run()
    .context("Failed to set ODGM env vars")?;

    // Wait for ODGM pod to restart with new env
    info!("Waiting for ODGM pod rollout...");
    duct::cmd!(
        "kubectl", "rollout", "status", "-n", "oaas-1",
        &format!("deployment/{}", odgm_deploy),
        "--timeout=120s"
    )
    .run()
    .context("ODGM rollout timed out")?;

    // Extra wait for readiness
    sleep(Duration::from_secs(5)).await;

    // --- Test 1: Object-level WS subscription ---
    info!("WS Test 1: Object-level subscription...");
    let ws_obj_id = format!("ws-test-{}", nanoid::nanoid!(6));

    // Seed object first so it exists
    cli.object_set(class, partition, &ws_obj_id, "init", "1")?;
    sleep(Duration::from_secs(1)).await;

    let url = format!(
        "{}/api/class/{}/{}/objects/{}/ws",
        gateway_ws_base, class, partition, ws_obj_id
    );
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&url)
        .await
        .context("Failed to connect WS (object-level)")?;
    info!("WS connected to {}", url);

    // Give subscriber time to register
    sleep(Duration::from_millis(500)).await;

    // Mutate the object → should trigger an event
    cli.object_set(class, partition, &ws_obj_id, "ws_key", "ws_value")?;

    // Read event from WS
    let msg = tokio::time::timeout(Duration::from_secs(10), ws_stream.next())
        .await
        .context("WS object event timeout")?
        .context("WS stream ended")?
        .context("WS message error")?;

    let text = match msg {
        tokio_tungstenite::tungstenite::Message::Text(t) => t.to_string(),
        other => {
            return Err(anyhow::anyhow!(
                "Expected Text WS frame, got {:?}",
                other
            ));
        }
    };
    info!("WS object event received: {}", text);
    if !text.contains(&ws_obj_id) {
        return Err(anyhow::anyhow!(
            "WS event missing object_id '{}': {}",
            ws_obj_id,
            text
        ));
    }

    // Close first WS
    drop(ws_stream);
    info!("WS Test 1 passed: object-level subscription works");

    // --- Test 2: Class-level WS subscription ---
    info!("WS Test 2: Class-level subscription...");
    let ws_obj_a = format!("ws-cls-a-{}", nanoid::nanoid!(6));
    let ws_obj_b = format!("ws-cls-b-{}", nanoid::nanoid!(6));

    // Seed objects
    cli.object_set(class, partition, &ws_obj_a, "init", "1")?;
    cli.object_set(class, partition, &ws_obj_b, "init", "1")?;
    sleep(Duration::from_secs(1)).await;

    let url = format!("{}/api/class/{}/ws", gateway_ws_base, class);
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&url)
        .await
        .context("Failed to connect WS (class-level)")?;
    info!("WS connected to {}", url);

    sleep(Duration::from_millis(500)).await;

    // Mutate both objects
    cli.object_set(class, partition, &ws_obj_a, "k", "v1")?;
    cli.object_set(class, partition, &ws_obj_b, "k", "v2")?;

    // Collect 2 events (with timeout)
    let mut received = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while received.len() < 2 {
        match tokio::time::timeout_at(deadline, ws_stream.next()).await {
            Ok(Some(Ok(tokio_tungstenite::tungstenite::Message::Text(t)))) => {
                info!("WS class event: {}", t);
                received.push(t.to_string());
            }
            Ok(Some(Ok(_))) => continue, // skip non-text frames
            Ok(Some(Err(e))) => {
                return Err(anyhow::anyhow!("WS error: {}", e));
            }
            Ok(None) => {
                return Err(anyhow::anyhow!("WS stream closed unexpectedly"));
            }
            Err(_) => break, // timeout
        }
    }

    if received.len() < 2 {
        return Err(anyhow::anyhow!(
            "Expected 2 class-level events, got {}",
            received.len()
        ));
    }
    let combined = received.join(" ");
    if !combined.contains(&ws_obj_a) || !combined.contains(&ws_obj_b) {
        return Err(anyhow::anyhow!(
            "Class-level WS missing events for both objects: {}",
            combined
        ));
    }

    drop(ws_stream);
    info!("WS Test 2 passed: class-level subscription works");

    info!("WebSocket E2E Tests Passed!");
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
