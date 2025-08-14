pub mod dev;
pub mod edge;
pub mod full;
pub mod knative;
pub mod manager;

pub use dev::DevTemplate;
pub use edge::EdgeTemplate;
pub use full::FullTemplate;
pub use knative::KnativeTemplate;
pub use manager::*;
