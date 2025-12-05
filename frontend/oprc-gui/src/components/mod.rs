//! UI components

pub mod layout;
pub mod raw_data_modal;
pub mod relative_time;
pub mod yaml_editor;

// Use split page components from the pages/ directory but alias the module name
// to avoid collision with the legacy `pages.rs` file during the transition.
#[path = "pages/mod.rs"]
pub mod pages_mod;

// Re-export for convenience
pub use layout::Navbar;
pub use pages_mod::{
    Deployments, Environments, Functions, Home, Objects, Packages, Settings,
    Topology,
};
pub use raw_data_modal::{RawDataModal, to_json_string};
pub use relative_time::{
    RelativeTime, RelativeTimeFromDt, RelativeTimeFromProto,
};
pub use yaml_editor::{DeploymentYamlEditor, ValidationResult, YamlEditor};
