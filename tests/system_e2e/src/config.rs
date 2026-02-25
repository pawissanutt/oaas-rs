use envconfig::Envconfig;

#[derive(Envconfig, Debug, Clone)]
pub struct TestConfig {
    #[envconfig(from = "OAAS_E2E_CLUSTER_NAME", default = "oaas-e2e")]
    pub cluster_name: String,

    #[envconfig(
        from = "IMAGE_PREFIX",
        default = "ghcr.io/pawissanutt/oaas-rs"
    )]
    pub image_prefix: String,

    #[envconfig(from = "OAAS_E2E_BUILD_PROFILE", default = "debug")]
    pub build_profile: String,

    #[envconfig(from = "IMAGE_TAG", default = "dev")]
    pub image_tag: String,

    #[envconfig(from = "OAAS_E2E_NO_CLEANUP", default = "false")]
    pub no_cleanup: bool,

    /// Skip the `just build` step (set to "true" when images are already built).
    #[envconfig(from = "OAAS_E2E_SKIP_BUILD", default = "false")]
    pub skip_build: bool,

    /// Skip tagging + pushing images to the local registry.
    #[envconfig(from = "OAAS_E2E_SKIP_LOAD", default = "false")]
    pub skip_load: bool,

    /// Skip the Helm deploy step (set to "true" when the system is already running).
    #[envconfig(from = "OAAS_E2E_SKIP_DEPLOY", default = "false")]
    pub skip_deploy: bool,

    /// Skip all infrastructure setup (kind + build + load + deploy).
    /// Useful when rerunning only the test scenarios against an existing cluster.
    #[envconfig(from = "OAAS_E2E_SKIP_INFRA", default = "false")]
    pub skip_infra: bool,
}
