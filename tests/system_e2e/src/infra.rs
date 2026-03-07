use crate::config::TestConfig;
use anyhow::{Context, Result};
use duct::cmd;
use tracing::{debug, error, info, instrument, warn};

/// Namespace where the WASM file server is deployed.
const WASM_SERVER_NS: &str = "oaas-1";
/// Name of the ConfigMap/Deployment/Service for the WASM file server.
const WASM_SERVER_NAME: &str = "wasm-file-server";

/// Returns "docker" if it is available on PATH, otherwise falls back to "podman".
fn container_cli() -> &'static str {
    let available = std::process::Command::new("docker")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if available { "docker" } else { "podman" }
}

pub struct InfraManager {
    config: TestConfig,
}

impl InfraManager {
    pub fn new(config: TestConfig) -> Self {
        Self { config }
    }

    #[instrument(skip(self))]
    pub fn setup_kind(&self) -> Result<()> {
        info!(
            "Checking if kind cluster '{}' exists...",
            self.config.cluster_name
        );

        let clusters = cmd!("kind", "get", "clusters").read()?;
        if clusters.lines().any(|l| l == self.config.cluster_name) {
            warn!(
                "Cluster '{}' already exists. Skipping creation.",
                self.config.cluster_name
            );
        } else {
            info!(
                "Creating kind cluster '{}' with local registry...",
                self.config.cluster_name
            );
            // Use the helper script to create cluster with local registry support
            // This is required for Knative and faster development cycles
            cmd!("tools/kind-with-registry.sh", &self.config.cluster_name)
                .run()
                .context("Failed to create kind cluster with registry")?;
        }
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn build_images(&self) -> Result<()> {
        info!(
            "Building images via just build {}...",
            self.config.build_profile
        );

        cmd!("just", "build", &self.config.build_profile)
            // .dir("../../") // Assume running from workspace root
            .run()
            .context("Failed to build images")?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub fn load_images(&self) -> Result<()> {
        let tag = &self.config.image_tag;

        // Short image names needed by the system
        let required = ["pm", "crm", "gateway", "router", "odgm"];
        let optional = ["echo-fn", "random-fn", "num-log-fn", "random-str-fn"];

        let registry = "localhost:5001";
        let cli = container_cli();
        info!("Using container CLI: {}", cli);

        // Verify registry is reachable
        info!("Checking local registry at {}...", registry);
        cmd!("curl", "-sf", format!("http://{}/v2/", registry))
            .stdout_null()
            .run()
            .context("Local registry is not reachable. Is the kind-registry container running?")?;

        // Prefixes to search for source images (in priority order)
        let prefixes = [
            self.config.image_prefix.clone(),
            "ghcr.io/pawissanutt/oaas-rs".to_string(),
            "harbor.129-114-108-168.nip.io/oaas".to_string(),
            "localhost:5001".to_string(),
        ];

        info!(
            "Tagging and pushing images to local registry {} (tag: {})...",
            registry, tag
        );

        let all_names: Vec<(&str, bool)> = required
            .iter()
            .map(|n| (*n, true))
            .chain(optional.iter().map(|n| (*n, false)))
            .collect();

        let mut pushed = 0usize;
        let mut skipped = Vec::new();

        for (name, is_required) in &all_names {
            let target = format!("{}/{}:{}", registry, name, tag);

            // Try each prefix to find the source image
            let mut found = false;
            for prefix in &prefixes {
                let source = format!("{}/{}:{}", prefix, name, tag);
                let exists = cmd!(cli, "image", "inspect", &source)
                    .stdout_null()
                    .stderr_null()
                    .run()
                    .is_ok();

                if exists {
                    debug!("Found {} -> tagging as {}", source, target);
                    cmd!(cli, "tag", &source, &target).run().context(
                        format!("Failed to tag {} -> {}", source, target),
                    )?;
                    cmd!(cli, "push", &target)
                        .run()
                        .context(format!("Failed to push {}", target))?;
                    pushed += 1;
                    found = true;
                    break;
                }
            }

            if !found {
                if *is_required {
                    error!(
                        "Required image '{}:{}' not found under any prefix {:?}. Run 'just build' first.",
                        name, tag, prefixes
                    );
                    return Err(anyhow::anyhow!(
                        "Required image '{}:{}' not found locally. Build images first with: just build",
                        name,
                        tag
                    ));
                } else {
                    warn!(
                        "Optional image '{}:{}' not found, skipping",
                        name, tag
                    );
                    skipped.push(*name);
                }
            }
        }

        // Verify critical images are in the registry
        info!("Verifying registry contents...");
        for name in &required {
            let url = format!("http://{}/v2/{}/tags/list", registry, name);
            let output = cmd!("curl", "-sf", &url).read();
            match output {
                Ok(body) if body.contains(tag) => {
                    debug!(
                        "Verified {}/{}:{} in registry",
                        registry, name, tag
                    );
                }
                _ => {
                    return Err(anyhow::anyhow!(
                        "Image '{}:{}' not found in registry after push. Something went wrong.",
                        name,
                        tag
                    ));
                }
            }
        }

        info!(
            "Successfully pushed {}/{} images to registry. Skipped: {:?}",
            pushed,
            all_names.len(),
            skipped
        );

        Ok(())
    }

    #[instrument(skip(self))]
    pub fn deploy_system(&self) -> Result<()> {
        info!("Deploying system components...");

        // Install NGINX Ingress (optional, but often needed for Gateway)
        // For now, let's assume the deploy script handles what it needs or we skip ingress if not strictly required for internal tests.
        // But the plan mentioned NGINX. Let's see if we can install it easily.
        // cmd!("kubectl", "apply", "-f", "https://raw.githubusercontent.com/kubernetes/ingress-nginx/main/deploy/static/provider/kind/deploy.yaml")
        //     .run()
        //     .context("Failed to install NGINX ingress")?;

        // Run the deploy script
        info!("Running k8s/charts/deploy.sh...");

        // Pass registry and tag to deploy script so Helm values match pushed images
        let registry = "localhost:5001";

        cmd!(
            "bash",
            "k8s/charts/deploy.sh",
            "deploy",
            "--registry",
            registry,
            "--tag",
            &self.config.image_tag,
            "--values-prefix",
            "system-e2e-"
        )
        // .dir("../../")
        .run()
        .context("Failed to run deploy script")?;

        // Wait for pods
        info!("Waiting for pods to be ready...");
        cmd!(
            "kubectl",
            "wait",
            "--for=condition=ready",
            "pod",
            "-l",
            "app.kubernetes.io/name in (oprc-pm, oprc-crm)",
            "--timeout=300s",
            "--all-namespaces"
        )
        .run()
        .context("Timeout waiting for pods")?;

        Ok(())
    }

    // #[instrument(skip(self))]
    // pub fn teardown(&self) -> Result<()> {
    //     info!("Tearing down cluster '{}'...", self.config.cluster_name);
    //     cmd!(
    //         "kind",
    //         "delete",
    //         "cluster",
    //         "--name",
    //         &self.config.cluster_name
    //     )
    //     .run()
    //     .context("Failed to delete cluster")?;
    //     Ok(())
    // }

    // #[instrument(skip(self))]
    // pub fn port_forward(
    //     &self,
    //     namespace: &str,
    //     service: &str,
    //     ports: &str,
    // ) -> Result<PortForwardGuard> {
    //     info!(
    //         "Port forwarding service {}/{} {}...",
    //         namespace, service, ports
    //     );
    //     let handle =
    //         cmd!("kubectl", "port-forward", "-n", namespace, service, ports)
    //             .start()
    //             .context("Failed to start port-forward")?;

    //     // Give it a moment to establish
    //     std::thread::sleep(std::time::Duration::from_secs(2));
    //     Ok(PortForwardGuard {
    //         handle,
    //         description: format!("{}/{} {}", namespace, service, ports),
    //     })
    // }

    #[instrument(skip(self))]
    pub fn check_requirements(&self) -> Result<()> {
        info!("Checking requirements...");
        let cli = container_cli();
        info!("Using container CLI: {}", cli);
        cmd!(cli, "--version")
            .run()
            .context("Neither docker nor podman is installed or in PATH")?;
        cmd!("kind", "--version")
            .run()
            .context("Kind is not installed or not in PATH")?;
        cmd!("kubectl", "version", "--client")
            .run()
            .context("Kubectl is not installed or not in PATH")?;
        Ok(())
    }

    /// Deploy an in-cluster nginx file server that serves WASM modules from a ConfigMap.
    ///
    /// Creates:
    /// 1. A ConfigMap containing the WASM binary (`--from-file`)
    /// 2. An nginx Deployment that mounts the ConfigMap at `/usr/share/nginx/html`
    /// 3. A ClusterIP Service exposing it on port 80
    ///
    /// Returns the in-cluster base URL, e.g. `http://wasm-file-server.oaas-1.svc.cluster.local`
    #[instrument(skip(self))]
    pub fn deploy_wasm_server(&self, wasm_binary_path: &str) -> Result<String> {
        info!(
            "Deploying in-cluster WASM file server (source: {})...",
            wasm_binary_path
        );

        let ns = WASM_SERVER_NS;
        let name = WASM_SERVER_NAME;

        // 1. Create ConfigMap from the WASM binary file
        // Delete existing first (ignore errors if it doesn't exist)
        let _ = cmd!(
            "kubectl",
            "delete",
            "configmap",
            name,
            "-n",
            ns,
            "--ignore-not-found"
        )
        .stdout_null()
        .stderr_null()
        .run();

        cmd!(
            "kubectl",
            "create",
            "configmap",
            name,
            "-n",
            ns,
            &format!("--from-file={}", wasm_binary_path)
        )
        .run()
        .context("Failed to create WASM ConfigMap")?;

        // 2. Apply nginx Deployment + Service
        let manifest = format!(
            r#"---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {name}
  namespace: {ns}
  labels:
    app: {name}
spec:
  replicas: 1
  selector:
    matchLabels:
      app: {name}
  template:
    metadata:
      labels:
        app: {name}
    spec:
      containers:
      - name: nginx
        image: nginx:alpine
        ports:
        - containerPort: 80
        volumeMounts:
        - name: wasm-files
          mountPath: /usr/share/nginx/html
          readOnly: true
      volumes:
      - name: wasm-files
        configMap:
          name: {name}
---
apiVersion: v1
kind: Service
metadata:
  name: {name}
  namespace: {ns}
spec:
  selector:
    app: {name}
  ports:
  - port: 80
    targetPort: 80
"#,
            name = name,
            ns = ns,
        );

        cmd!("kubectl", "apply", "-f", "-")
            .stdin_bytes(manifest.as_bytes())
            .run()
            .context("Failed to apply WASM server Deployment/Service")?;

        // 3. Wait for the pod to be ready
        info!("Waiting for WASM file server pod...");
        cmd!(
            "kubectl",
            "wait",
            "--for=condition=ready",
            "pod",
            "-l",
            &format!("app={}", name),
            "-n",
            ns,
            "--timeout=120s"
        )
        .run()
        .context("Timeout waiting for WASM file server pod")?;

        let base_url = format!("http://{}.{}.svc.cluster.local", name, ns);
        info!("WASM file server ready at {}", base_url);
        Ok(base_url)
    }

    /// Clean up the in-cluster WASM file server resources.
    #[allow(dead_code)]
    #[instrument(skip(self))]
    pub fn cleanup_wasm_server(&self) {
        info!("Cleaning up WASM file server...");
        let ns = WASM_SERVER_NS;
        let name = WASM_SERVER_NAME;
        for kind in &["service", "deployment", "configmap"] {
            let _ = cmd!(
                "kubectl",
                "delete",
                kind,
                name,
                "-n",
                ns,
                "--ignore-not-found"
            )
            .stdout_null()
            .stderr_null()
            .run();
        }
    }
}
