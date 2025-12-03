use envconfig::Envconfig;
use std::{env, error::Error, fmt::Display, str::FromStr};

#[derive(Envconfig, Clone, Debug)]
pub struct Config {
    #[envconfig(from = "HTTP_PORT", default = "8080")]
    pub http_port: u16,
    #[envconfig(from = "REQUEST_TIMEOUT_MS", default = "30000")]
    pub request_timeout_ms: u64,
    /// Maximum payload size in bytes (default: 50MB)
    #[envconfig(from = "MAX_PAYLOAD_BYTES", default = "52428800")]
    pub max_payload_bytes: usize,
    #[envconfig(from = "RETRY_ATTEMPTS", default = "0")]
    pub retry_attempts: u32,
    #[envconfig(from = "RETRY_BACKOFF_MS", default = "25")]
    pub retry_backoff_ms: u64,
    // Optional: either "json" or "plain"/"text"; defaults handled in tracing setup
    #[envconfig(from = "LOG_FORMAT")]
    pub log_format: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParseEnvError {
    pub name: String,
    pub value: String,
}

impl Display for ParseEnvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ParseError {{ name: '{}', value: '{}' }}",
            self.name, self.value
        )
    }
}

impl Error for ParseEnvError {}

fn read_env<T>(name: &str) -> Result<Option<T>, ParseEnvError>
where
    T: FromStr,
    <T as FromStr>::Err: Display,
{
    match env::var(name) {
        Ok(val) => match val.parse::<T>() {
            Ok(v) => Ok(Some(v)),
            Err(_e) => Err(ParseEnvError {
                name: name.to_string(),
                value: val,
            }),
        },
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(os)) => Err(ParseEnvError {
            name: name.to_string(),
            value: format!("non-unicode: {:?}", os),
        }),
    }
}

fn read_with_prefix<T>(base: &str) -> Result<Option<T>, ParseEnvError>
where
    T: FromStr,
    <T as FromStr>::Err: Display,
{
    let prefixed = format!("OPRC_GW_{}", base);
    if let Some(v) = read_env::<T>(&prefixed)? {
        return Ok(Some(v));
    }
    read_env::<T>(base)
}

impl Config {
    pub fn load_from_env() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let http_port = read_with_prefix::<u16>("HTTP_PORT")?.unwrap_or(8080);
        let request_timeout_ms =
            read_with_prefix::<u64>("REQUEST_TIMEOUT_MS")?.unwrap_or(30000);
        let max_payload_bytes =
            read_with_prefix::<usize>("MAX_PAYLOAD_BYTES")?.unwrap_or(4194304);
        let retry_attempts =
            read_with_prefix::<u32>("RETRY_ATTEMPTS")?.unwrap_or(0);
        let retry_backoff_ms =
            read_with_prefix::<u64>("RETRY_BACKOFF_MS")?.unwrap_or(25);
        let log_format = read_with_prefix::<String>("LOG_FORMAT")?;
        Ok(Config {
            http_port,
            request_timeout_ms,
            max_payload_bytes,
            retry_attempts,
            retry_backoff_ms,
            log_format,
        })
    }
}
