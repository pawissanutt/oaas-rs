pub mod proto {
    pub mod common {
        tonic::include_proto!("oaas.common");
    }
    pub mod package {
        tonic::include_proto!("oaas.package");
    }
    pub mod deployment {
        tonic::include_proto!("oaas.deployment");
    }
    pub mod runtime {
        tonic::include_proto!("oaas.runtime");
    }
    pub mod health {
        tonic::include_proto!("oaas.health");
    }
}

pub mod client;
pub mod server;
pub mod types;

// Re-export all proto types at the crate root for convenience
pub use proto::common::*;
pub use proto::deployment::*;
pub use proto::health::*;
pub use proto::package::*;
pub use proto::runtime::*;
