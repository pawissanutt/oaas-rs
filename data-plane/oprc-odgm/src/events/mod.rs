pub mod config;
pub mod manager;
pub mod processor;
pub mod types;

#[cfg(test)]
mod tests;

pub use config::EventConfig;
pub use manager::EventManager;
pub use processor::TriggerProcessor;
pub use types::*;
