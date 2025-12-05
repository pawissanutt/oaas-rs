//! Page component module - individual page components split for maintainability.

pub mod deployments;
pub mod environments;
pub mod functions;
pub mod home;
pub mod objects;
pub mod packages;
pub mod settings;
pub mod topology;

pub use deployments::Deployments;
pub use environments::Environments;
pub use functions::Functions;
pub use home::Home;
pub use objects::Objects;
pub use packages::Packages;
pub use settings::Settings;
pub use topology::Topology;

// Re-export settings utilities for external use
#[allow(unused_imports)]
pub use settings::{GuiSettings, apply_theme, load_settings, save_settings};
