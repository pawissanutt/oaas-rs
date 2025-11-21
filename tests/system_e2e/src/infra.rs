use crate::config::TestConfig;
use anyhow::{Context, Result};
use duct::cmd;
use tracing::{debug, info, instrument, warn};

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
        let images = vec![
            format!(
                "{}/pm:{}",
                self.config.image_prefix, self.config.image_tag
            ),
            format!(
                "{}/crm:{}",
                self.config.image_prefix, self.config.image_tag
            ),
            format!(
                "{}/gateway:{}",
                self.config.image_prefix, self.config.image_tag
            ),
            format!(
                "{}/odgm:{}",
                self.config.image_prefix, self.config.image_tag
            ),
            format!(
                "{}/router:{}",
                self.config.image_prefix, self.config.image_tag
            ),
            format!(
                "{}/echo-fn:{}",
                self.config.image_prefix, self.config.image_tag
            ),
            format!(
                "{}/random-fn:{}",
                self.config.image_prefix, self.config.image_tag
            ),
            format!(
                "{}/num-log-fn:{}",
                self.config.image_prefix, self.config.image_tag
            ),
            format!(
                "{}/random-str-fn:{}",
                self.config.image_prefix, self.config.image_tag
            ),
        ];

        // Check if local registry is available (localhost:5001)
        // We assume if the cluster was created with registry support, port 5001 is open
        let registry = "localhost:5001";

        info!(
            "Tagging and pushing images to local registry {}...",
            registry
        );

        for image in images {
            // image is like ghcr.io/pawissanutt/oaas-rs/pm:dev-alpha
            // we want localhost:5001/oaas-rs/pm:dev-alpha
            let image_name = image.split('/').last().unwrap();
            let target_image = format!("{}/oaas-rs/{}", registry, image_name);

            debug!("Pushing {} -> {}", image, target_image);
            cmd!("docker", "tag", &image, &target_image)
                .run()
                .context("Failed to tag image")?;

            cmd!("docker", "push", &target_image)
                .run()
                .context("Failed to push image to registry")?;
        }

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

        // Pass registry to deploy script
        let registry = "localhost:5001";

        cmd!(
            "bash",
            "k8s/charts/deploy.sh",
            "deploy",
            "--registry",
            registry,
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
        cmd!("docker", "--version")
            .run()
            .context("Docker is not installed or not in PATH")?;
        cmd!("kind", "--version")
            .run()
            .context("Kind is not installed or not in PATH")?;
        cmd!("kubectl", "version", "--client")
            .run()
            .context("Kubectl is not installed or not in PATH")?;
        Ok(())
    }
}

// pub struct PortForwardGuard {
//     handle: duct::Handle,
//     description: String,
// }

// impl Drop for PortForwardGuard {
//     fn drop(&mut self) {
//         info!("Stopping port-forward for {}...", self.description);
//         if let Err(e) = self.handle.kill() {
//             warn!("Failed to kill port-forward process: {}", e);
//         }
//     }
// }
