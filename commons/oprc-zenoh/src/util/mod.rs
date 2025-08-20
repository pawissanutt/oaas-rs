//! Utilities for managing Zenoh queryables and subscribers with concurrent handling.
//!
//! This module provides abstractions for creating managed Zenoh queryables and subscribers
//! that automatically handle incoming messages/queries using configurable concurrency.
//!
//! # Examples
//!
//! ## Creating a managed queryable
//!
//! ```rust,no_run
//! use oprc_zenoh::util::{declare_managed_queryable, ManagedConfig, Handler};
//! use zenoh::query::Query;
//!
//! #[derive(Clone)]
//! struct MyHandler;
//!
//! #[async_trait::async_trait]
//! impl Handler<Query> for MyHandler {
//!     async fn handle(&self, query: Query) {
//!         query.reply("my_key", "Hello World!").await.unwrap();
//!     }
//! }
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//! let session = zenoh::open(zenoh::Config::default()).await?;
//! let config = ManagedConfig::new("my/service/**", 4, 1000);
//! let queryable = declare_managed_queryable(&session, config, MyHandler).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Error handling with thiserror
//!
//! The module uses `thiserror` for better error handling:
//!
//! ```rust,no_run
//! use oprc_zenoh::util::{declare_managed_queryable, ManagedConfig, ZenohError};
//!
//! # #[tokio::main]
//! # async fn main() {
//! let config = ManagedConfig::new("", 0, 100); // Invalid config
//! match config.validate() {
//!     Err(ZenohError::Config(msg)) => {
//!         eprintln!("Configuration error: {}", msg);
//!     }
//!     _ => {}
//! }
//! # }
//! ```

use flume::Receiver;
use thiserror::Error;
use zenoh::{
    Session,
    pubsub::Subscriber,
    query::{Query, Queryable},
    sample::Sample,
};

/// Default channel size for unbounded channels
const UNBOUNDED_CHANNEL: usize = 0;

/// Type alias for common error result
type ZenohResult<T> = Result<T, ZenohError>;

/// Type alias for managed queryable
pub type ManagedQueryable = Queryable<Receiver<Query>>;

/// Type alias for managed subscriber
pub type ManagedSubscriber = Subscriber<Receiver<Sample>>;

/// Custom error type for Zenoh operations
#[derive(Error, Debug)]
pub enum ZenohError {
    /// Error during Zenoh session operations
    #[error("Zenoh session error: {0}")]
    Session(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Zenoh API error
    #[error("Zenoh API error: {source}")]
    ZenohApi {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl ZenohError {
    /// Create a new session error
    pub fn session(msg: impl Into<String>) -> Self {
        Self::Session(msg.into())
    }

    /// Create a new configuration error
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// Create a new Zenoh API error from any error
    pub fn zenoh_api<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::ZenohApi {
            source: Box::new(err),
        }
    }
}

/// Trait for handling incoming messages asynchronously
#[async_trait::async_trait]
pub trait Handler<T>: Send + Clone {
    /// Handle an incoming message
    async fn handle(&self, input: T);
}

/// Configuration for managed Zenoh declarations
#[derive(Debug, Clone)]
pub struct ManagedConfig {
    /// Key expression for the declaration
    pub key: String,
    /// Number of concurrent worker tasks
    pub concurrency: usize,
    /// Channel size (0 for unbounded)
    pub channel_size: usize,
}

impl ManagedConfig {
    /// Create a new configuration
    pub fn new(
        key: impl Into<String>,
        concurrency: usize,
        channel_size: usize,
    ) -> Self {
        Self {
            key: key.into(),
            concurrency,
            channel_size,
        }
    }

    /// Create a configuration with unbounded channel
    pub fn unbounded(key: impl Into<String>, concurrency: usize) -> Self {
        Self::new(key, concurrency, UNBOUNDED_CHANNEL)
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), ZenohError> {
        if self.concurrency == 0 {
            return Err(ZenohError::config(
                "Concurrency must be greater than 0",
            ));
        }
        if self.key.is_empty() {
            return Err(ZenohError::config("Key cannot be empty"));
        }
        Ok(())
    }
}

/// Create a channel based on the specified size
fn create_bounded_channel<T>(
    size: usize,
) -> (flume::Sender<T>, flume::Receiver<T>)
where
    T: Send,
{
    if size == UNBOUNDED_CHANNEL {
        flume::unbounded()
    } else {
        flume::bounded(size)
    }
}

/// Spawn worker tasks for handling incoming messages
fn spawn_workers<T, H>(
    receiver: flume::Receiver<T>,
    handler: H,
    key: &str,
    concurrency: usize,
) where
    T: Send + 'static,
    H: Handler<T> + 'static,
{
    for worker_id in 0..concurrency {
        let worker_rx = receiver.clone();
        let worker_key = key.to_string();
        let worker_handler = handler.clone();

        tokio::spawn(async move {
            tracing::debug!(
                "Worker {} for '{}' started",
                worker_id,
                worker_key
            );

            loop {
                match worker_rx.recv_async().await {
                    Ok(message) => {
                        worker_handler.handle(message).await;
                    }
                    Err(err) => {
                        tracing::debug!(
                            "Worker {} for '{}' stopped: {}",
                            worker_id,
                            worker_key,
                            err
                        );
                        break;
                    }
                }
            }
        });
    }
}

/// Declare a managed queryable with concurrent message handling
///
/// # Arguments
/// * `session` - The Zenoh session
/// * `config` - Configuration for the managed queryable
/// * `handler` - Handler for processing queries
///
/// # Returns
/// A managed queryable that automatically handles incoming queries
pub async fn declare_managed_queryable<H>(
    session: &Session,
    config: ManagedConfig,
    handler: H,
) -> ZenohResult<ManagedQueryable>
where
    H: Handler<Query> + 'static,
{
    config.validate()?;

    let channel = create_bounded_channel(config.channel_size);
    let queryable = session
        .declare_queryable(&config.key)
        .complete(true)
        .with(channel)
        .await
        .map_err(|e| ZenohError::ZenohApi { source: e })?;

    tracing::info!(
        "Queryable '{}' declared with {} worker threads",
        config.key,
        config.concurrency
    );

    spawn_workers(
        queryable.handler().clone(),
        handler,
        &config.key,
        config.concurrency,
    );

    Ok(queryable)
}

/// Declare a managed subscriber with concurrent message handling
///
/// # Arguments
/// * `session` - The Zenoh session
/// * `config` - Configuration for the managed subscriber
/// * `handler` - Handler for processing samples
///
/// # Returns
/// A managed subscriber that automatically handles incoming samples
pub async fn declare_managed_subscriber<H>(
    session: &Session,
    config: ManagedConfig,
    handler: H,
) -> ZenohResult<ManagedSubscriber>
where
    H: Handler<Sample> + 'static,
{
    config.validate()?;

    let channel = create_bounded_channel(config.channel_size);
    let subscriber = session
        .declare_subscriber(&config.key)
        .with(channel)
        .await
        .map_err(|e| ZenohError::ZenohApi { source: e })?;

    tracing::info!(
        "Subscriber '{}' declared with {} worker threads",
        config.key,
        config.concurrency
    );

    spawn_workers(
        subscriber.handler().clone(),
        handler,
        &config.key,
        config.concurrency,
    );

    Ok(subscriber)
}

/// Legacy function for backward compatibility
#[deprecated(
    since = "0.1.0",
    note = "Use declare_managed_queryable with ManagedConfig instead"
)]
pub async fn declare_managed_queryable_legacy<H>(
    z_session: &Session,
    key: String,
    handler: H,
    concurrency: usize,
    channel_size: usize,
) -> Result<ManagedQueryable, Box<dyn std::error::Error + Send + Sync>>
where
    H: Handler<Query> + 'static,
{
    let config = ManagedConfig::new(key, concurrency, channel_size);
    declare_managed_queryable(z_session, config, handler)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
}

/// Legacy function for backward compatibility
#[deprecated(
    since = "0.1.0",
    note = "Use declare_managed_subscriber with ManagedConfig instead"
)]
pub async fn declare_managed_subscriber_legacy<H>(
    z_session: &Session,
    key: String,
    handler: H,
    concurrency: usize,
    channel_size: usize,
) -> Result<ManagedSubscriber, Box<dyn std::error::Error + Send + Sync>>
where
    H: Handler<Sample> + 'static,
{
    let config = ManagedConfig::new(key, concurrency, channel_size);
    declare_managed_subscriber(z_session, config, handler)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
}
