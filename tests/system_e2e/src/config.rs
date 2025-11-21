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

    #[envconfig(from = "IMAGE_TAG", default = "dev-alpha")]
    pub image_tag: String,

    #[envconfig(from = "OAAS_E2E_NO_CLEANUP", default = "false")]
    pub no_cleanup: bool,
}
