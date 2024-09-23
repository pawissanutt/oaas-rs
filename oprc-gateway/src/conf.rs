use envconfig::Envconfig;

#[derive(Envconfig)]
pub struct Config {
    #[envconfig(from = "HTTP_PORT", default = "8080")]
    pub http_port: u16,
    #[envconfig(from = "OPRC_PM_URI", default = "localhost:8081")]
    pub pm_uri: String,
}
