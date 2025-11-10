#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FsmAction {
    ApplyWorkload,
    EnsureMetricsTargets,
    DeleteOrphans(Vec<String>),
    EnforceReplicas(u32),
}
