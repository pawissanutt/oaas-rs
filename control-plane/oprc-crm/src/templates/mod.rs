pub mod dev;
pub mod edge;
pub mod k8s_deployment;
pub mod knative;
pub mod manager;
pub mod odgm;

pub use dev::DevTemplate;
pub use edge::EdgeTemplate;
pub use k8s_deployment::K8sDeploymentTemplate;
pub use knative::KnativeTemplate;
pub use manager::*;
pub use odgm::*;
