// Basic E2E test flow
// Tests: Package apply -> Deployment apply -> Function invocation

use anyhow::Result;
use kube::Client;
use std::path::PathBuf;
use system_e2e::*;

/// E2E test harness
struct E2ETestHarness {
    cluster_name: String,
    repo_root: PathBuf,
    cleanup_on_drop: bool,
}

impl E2ETestHarness {
    fn new() -> Result<Self> {
        Ok(Self {
            cluster_name: CLUSTER_NAME.to_string(),
            repo_root: repo_root()?,
            cleanup_on_drop: !should_skip_cleanup(),
        })
    }

    async fn setup(&self) -> Result<()> {
        tracing::info!("=== E2E Test Setup Phase ===");
        
        // Check prerequisites
        setup::check_prerequisites()?;
        
        // Create Kind cluster
        setup::create_kind_cluster(&self.cluster_name)?;
        
        // Build images
        setup::build_images(&self.repo_root)?;
        
        // Load images into Kind
        setup::load_all_images(&self.cluster_name, REQUIRED_IMAGES)?;
        
        // Deploy OaaS
        setup::deploy_oaas(&self.repo_root)?;
        
        // Wait for control plane to be ready
        let client = Client::try_default().await?;
        k8s_helpers::wait_for_oaas_control_plane(&client, "oaas", 300).await?;
        
        tracing::info!("=== Setup Complete ===");
        Ok(())
    }

    async fn teardown(&self) -> Result<()> {
        if !self.cleanup_on_drop {
            tracing::info!("Skipping cleanup (E2E_SKIP_CLEANUP is set)");
            return Ok(());
        }
        
        tracing::info!("=== E2E Test Teardown Phase ===");
        
        // Undeploy OaaS
        let _ = setup::undeploy_oaas(&self.repo_root);
        
        // Delete Kind cluster
        setup::delete_kind_cluster(&self.cluster_name)?;
        
        tracing::info!("=== Teardown Complete ===");
        Ok(())
    }
}

impl Drop for E2ETestHarness {
    fn drop(&mut self) {
        if self.cleanup_on_drop {
            // Best-effort cleanup
            let cluster_name = self.cluster_name.clone();
            let _ = std::thread::spawn(move || {
                let _ = setup::delete_kind_cluster(&cluster_name);
            });
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore] // Ignore by default as it requires Kind and Docker
async fn test_basic_e2e_flow() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();
    
    tracing::info!("Starting E2E test: basic_flow");
    
    let harness = E2ETestHarness::new()?;
    
    // Setup infrastructure
    harness.setup().await?;
    
    // Run test scenario
    let result = run_basic_flow_scenario().await;
    
    // Teardown
    harness.teardown().await?;
    
    // Return test result
    result
}

async fn run_basic_flow_scenario() -> Result<()> {
    tracing::info!("=== Running Basic Flow Scenario ===");
    
    let client = Client::try_default().await?;
    
    // Initialize CLI wrapper
    let mut cli = cli::CliWrapper::new()?;
    
    // Configure PM URL (assuming port-forward or known endpoint)
    // In a real scenario, you might need to set up port forwarding
    cli.configure_pm("http://localhost:8080")?;
    
    // Get fixture paths
    let fixtures_dir = repo_root()?.join("tests/system_e2e/src/fixtures");
    let package_file = fixtures_dir.join("example_package.yaml");
    let deployment_file = fixtures_dir.join("example_deployment.yaml");
    
    // Step 1: Apply package
    tracing::info!("Step 1: Applying package");
    let pkg_output = cli.pkg_apply(&package_file)?;
    tracing::info!("Package applied: {}", pkg_output);
    
    // Verify package exists in PM
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let pkg_info = cli.pkg_get("e2e-test-pkg")?;
    assert!(pkg_info.get("name").is_some(), "Package not found in PM");
    tracing::info!("Package verified in PM");
    
    // Step 2: Apply deployment
    tracing::info!("Step 2: Applying deployment");
    let dep_output = cli.dep_apply(&deployment_file)?;
    tracing::info!("Deployment applied: {}", dep_output);
    
    // Wait for ODGM pods to appear
    tracing::info!("Waiting for ODGM pods...");
    k8s_helpers::wait_for_odgm_pods(
        &client,
        "oaas-1", // Target environment namespace
        "TestRecord",
        1,
        120, // 2 minutes timeout
    )
    .await?;
    tracing::info!("ODGM pods are ready");
    
    // Step 3: Invoke function
    tracing::info!("Step 3: Invoking function");
    let test_payload = r#"{"test": "data"}"#;
    let invoke_output = cli.invoke(
        "TestRecord",
        "test-instance-1",
        "echo",
        Some(test_payload),
    )?;
    tracing::info!("Function invoked: {}", invoke_output);
    
    // Verify response contains expected data
    assert!(
        invoke_output.contains("test") || invoke_output.contains("data"),
        "Function response did not contain expected data"
    );
    
    tracing::info!("=== Basic Flow Scenario Complete ===");
    Ok(())
}

#[test]
fn test_harness_creation() {
    let harness = E2ETestHarness::new();
    assert!(harness.is_ok());
}
