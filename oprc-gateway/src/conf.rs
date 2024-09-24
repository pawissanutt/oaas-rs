use envconfig::Envconfig;

#[derive(Envconfig, Clone, Debug)]
pub struct Config {
    #[envconfig(from = "HTTP_PORT", default = "8080")]
    pub http_port: u16,
    #[envconfig(from = "OPRC_PM_URI", default = "localhost:8081")]
    pub pm_uri: String,
    #[envconfig(from = "OPRC_PRINT_POOL_INTERVAL", default = "5")]
    pub print_pool_state_interval: u32,
    #[envconfig(from = "OPRC_MAX_POOL_SIZE", default = "64")]
    pub max_pool_size: u32,
}
