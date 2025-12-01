use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageResponse {
    pub id: String,
    pub status: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentResponse {
    pub id: String,
    pub status: String,
    pub message: Option<String>,
}

// Reduce redundancy by using gRPC-generated types for ClassRuntime and related
pub type ClassRuntime = oprc_grpc::proto::runtime::ClassRuntimeSummary;
pub type ClassRuntimeStatus = oprc_grpc::proto::runtime::ClassRuntimeStatus;
pub type DeploymentPhase = oprc_grpc::proto::runtime::DeploymentPhase;
pub type NfrCompliance = oprc_grpc::proto::runtime::NfrCompliance;
pub type ResourceReference = oprc_grpc::proto::deployment::ResourceReference;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentStatus {
    pub id: String,
    pub status: String,
    pub phase: String,
    pub message: Option<String>,
    pub last_updated: String,
}
