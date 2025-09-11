use serde::Serialize;

// REST-facing DTOs with string enums

#[derive(Debug, Clone, Serialize)]
pub struct ApiClassRuntimeStatus {
    pub condition: String, // PascalCase: Pending, Running, Down, Deleted, Deploying, Unknown
    pub phase: String, // SCREAMING_SNAKE_CASE: UNKNOWN, TEMPLATE_SELECTION, ...
    pub message: Option<String>,
    pub last_updated: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiClassRuntime {
    pub id: String,
    pub deployment_unit_id: String,
    pub package_name: String,
    pub class_key: String,
    pub target_environment: String,
    pub cluster_name: Option<String>,
    pub status: Option<ApiClassRuntimeStatus>,
    pub nfr_compliance: Option<oprc_grpc::proto::runtime::NfrCompliance>,
    pub resource_refs: Vec<oprc_grpc::proto::deployment::ResourceReference>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<oprc_grpc::proto::runtime::ClassRuntimeSummary> for ApiClassRuntime {
    fn from(src: oprc_grpc::proto::runtime::ClassRuntimeSummary) -> Self {
        Self {
            id: src.id,
            deployment_unit_id: src.deployment_unit_id,
            package_name: src.package_name,
            class_key: src.class_key,
            target_environment: src.target_environment,
            cluster_name: src.cluster_name,
            status: src.status.map(|s| ApiClassRuntimeStatus::from(s)),
            nfr_compliance: src.nfr_compliance,
            resource_refs: src.resource_refs,
            created_at: src.created_at,
            updated_at: src.updated_at,
        }
    }
}

impl From<oprc_grpc::proto::runtime::ClassRuntimeStatus>
    for ApiClassRuntimeStatus
{
    fn from(s: oprc_grpc::proto::runtime::ClassRuntimeStatus) -> Self {
        Self {
            condition: condition_to_string(s.condition),
            phase: phase_to_string(s.phase),
            message: s.message,
            last_updated: s.last_updated,
        }
    }
}

fn condition_to_string(v: i32) -> String {
    use oprc_grpc::proto::runtime::DeploymentCondition as C;
    match C::try_from(v).ok() {
        Some(C::Pending) => "Pending".to_string(),
        Some(C::Deploying) => "Deploying".to_string(),
        Some(C::Running) => "Running".to_string(),
        Some(C::Down) => "Down".to_string(),
        Some(C::Deleted) => "Deleted".to_string(),
        _ => "Unknown".to_string(),
    }
}

fn phase_to_string(v: i32) -> String {
    use oprc_grpc::proto::runtime::DeploymentPhase as P;
    match P::try_from(v).ok() {
        Some(P::ResourceProvisioning) => "RESOURCE_PROVISIONING".to_string(),
        // PHASE_RUNNING (covers previously ENFORCEMENT/COMPLETED)
        Some(P::PhaseRunning) => "RUNNING".to_string(),
        Some(P::Failed) => "FAILED".to_string(),
        _ => "UNKNOWN".to_string(),
    }
}
