pub mod api;
pub mod config;
pub mod crm;
pub mod errors;
pub mod models;
pub mod server;
pub mod services;
pub mod storage;
pub mod bootstrap;

// Re-export commonly used types and functions, but avoid conflicts
pub use config::*;
pub use errors::*;
pub use models::*;
pub use server::{ApiServer, AppState};
pub use storage::*;

// Re-export services with specific names to avoid conflicts
pub use services::{DeploymentService, PackageService};

// Re-export CRM components
pub use crm::{CrmClient, CrmManager};

// Re-export API components (handlers are typically not re-exported at crate level)
pub use api::create_middleware_stack;
// Re-export bootstrap helpers
pub use bootstrap::build_api_server_from_env;
