//! UI components

pub mod layout;
pub mod pages;

// Re-export for convenience
pub use layout::Navbar;
pub use pages::{Deployments, Home, Invoke, Objects, Packages, Topology};
