//! UI components

pub mod layout;

// Use split page components from the pages/ directory but alias the module name
// to avoid collision with the legacy `pages.rs` file during the transition.
#[path = "pages/mod.rs"]
pub mod pages_mod;

// Re-export for convenience
pub use layout::Navbar;
pub use pages_mod::{Deployments, Home, Objects, Packages, Topology};
