use envconfig::Envconfig;

#[derive(Envconfig, Clone, Debug)]
pub struct CrmConfig {
    #[envconfig(from = "OPRC_CRM_PROFILE", default = "dev")]
    pub profile: String,

    #[envconfig(from = "HTTP_PORT", default = "8088")]
    pub http_port: u16,

    #[envconfig(from = "GRPC_PORT", default = "7088")]
    pub grpc_port: u16,

    #[envconfig(from = "OPRC_CRM_K8S_NAMESPACE", default = "default")]
    pub k8s_namespace: String,

    /// Enable gRPC mTLS (profile default: false in dev/edge, true in full)
    /// Env: OPRC_CRM_SECURITY_MTLS
    #[envconfig(from = "OPRC_CRM_SECURITY_MTLS")]
    pub security_mtls: Option<bool>,

    #[envconfig(nested)]
    pub features: FeaturesConfig,

    #[envconfig(nested)]
    pub enforcement: EnforcementConfig,
}

#[derive(Envconfig, Clone, Debug, Default)]
pub struct FeaturesConfig {
    /// If Some, env explicitly set; otherwise, profile defaults apply
    #[envconfig(from = "OPRC_CRM_FEATURES_NFR_ENFORCEMENT")]
    pub nfr_enforcement: Option<bool>,
    #[envconfig(from = "OPRC_CRM_FEATURES_HPA")]
    pub hpa: Option<bool>,
    #[envconfig(from = "OPRC_CRM_FEATURES_KNATIVE")]
    pub knative: Option<bool>,
    #[envconfig(from = "OPRC_CRM_FEATURES_PROMETHEUS")]
    pub prometheus: Option<bool>,
    #[envconfig(from = "OPRC_CRM_FEATURES_LEADER_ELECTION")]
    pub leader_election: Option<bool>,
    #[envconfig(from = "OPRC_CRM_FEATURES_ODGM")]
    pub odgm_sidecar: Option<bool>,
}

#[derive(Envconfig, Clone, Debug)]
pub struct EnforcementConfig {
    #[envconfig(from = "OPRC_CRM_ENFORCEMENT_COOLDOWN_SECS", default = "120")]
    pub cooldown_secs: u64,
    #[envconfig(
        from = "OPRC_CRM_ENFORCEMENT_MAX_REPLICA_DELTA",
        default = "30"
    )]
    pub max_replica_delta_pct: u8,
    #[envconfig(from = "OPRC_CRM_LIMITS_MAX_REPLICAS", default = "20")]
    pub max_replicas: u32,
}

impl CrmConfig {
    /// Apply profile → defaults mapping, while respecting explicit env overrides.
    ///
    /// Rules:
    /// - dev: nfr_enforcement=false, hpa=false, knative=false, prometheus=false, leader_election=false, mTLS=false
    /// - edge: nfr_enforcement=false, hpa=true,  knative=false, prometheus=true,  leader_election=true,  mTLS=false
    /// - full: nfr_enforcement=true,  hpa=true,  knative=false, prometheus=true,  leader_election=true,  mTLS=true
    pub fn apply_profile_defaults(mut self) -> Self {
        let (def_nfr, def_hpa, def_kn, def_prom, def_le, def_mtls, def_odgm) =
            match self.profile.as_str() {
                "edge" => (false, true, false, true, true, false, false),
                "full" | "prod" | "production" => {
                    (true, true, false, true, true, true, true)
                }
                _ /* dev */ => (false, false, false, false, false, false, false),
            };

        // Only set when not explicitly provided via env
        if self.features.nfr_enforcement.is_none() {
            self.features.nfr_enforcement = Some(def_nfr);
        }
        if self.features.hpa.is_none() {
            self.features.hpa = Some(def_hpa);
        }
        if self.features.knative.is_none() {
            self.features.knative = Some(def_kn);
        }
        if self.features.prometheus.is_none() {
            self.features.prometheus = Some(def_prom);
        }
        if self.features.leader_election.is_none() {
            self.features.leader_election = Some(def_le);
        }
        if self.features.odgm_sidecar.is_none() {
            self.features.odgm_sidecar = Some(def_odgm);
        }
        if self.security_mtls.is_none() {
            self.security_mtls = Some(def_mtls);
        }

        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(profile: &str) -> CrmConfig {
        CrmConfig {
            profile: profile.to_string(),
            http_port: 8088,
            grpc_port: 7088,
            k8s_namespace: "default".into(),
            security_mtls: None,
            features: FeaturesConfig::default(),
            enforcement: EnforcementConfig {
                cooldown_secs: 120,
                max_replica_delta_pct: 30,
                max_replicas: 20,
            },
        }
    }

    #[test]
    fn profile_defaults_dev() {
        let cfg = base("dev").apply_profile_defaults();
        assert_eq!(cfg.features.nfr_enforcement, Some(false));
        assert_eq!(cfg.features.hpa, Some(false));
        assert_eq!(cfg.features.knative, Some(false));
        assert_eq!(cfg.features.prometheus, Some(false));
        assert_eq!(cfg.features.leader_election, Some(false));
        assert_eq!(cfg.features.odgm_sidecar, Some(false));
        assert_eq!(cfg.security_mtls, Some(false));
    }

    #[test]
    fn profile_defaults_edge() {
        let cfg = base("edge").apply_profile_defaults();
        assert_eq!(cfg.features.nfr_enforcement, Some(false));
        assert_eq!(cfg.features.hpa, Some(true));
        assert_eq!(cfg.features.knative, Some(false));
        assert_eq!(cfg.features.prometheus, Some(true));
        assert_eq!(cfg.features.leader_election, Some(true));
        assert_eq!(cfg.features.odgm_sidecar, Some(false));
        assert_eq!(cfg.security_mtls, Some(false));
    }

    #[test]
    fn profile_defaults_full() {
        for p in ["full", "prod", "production"] {
            let cfg = base(p).apply_profile_defaults();
            assert_eq!(cfg.features.nfr_enforcement, Some(true));
            assert_eq!(cfg.features.hpa, Some(true));
            assert_eq!(cfg.features.knative, Some(false));
            assert_eq!(cfg.features.prometheus, Some(true));
            assert_eq!(cfg.features.leader_election, Some(true));
            assert_eq!(cfg.features.odgm_sidecar, Some(true));
            assert_eq!(cfg.security_mtls, Some(true));
        }
    }

    #[test]
    fn profile_defaults_respect_env_overrides() {
        let mut cfg = base("full");
        cfg.features.hpa = Some(false); // explicitly disabled via env
        cfg.security_mtls = Some(false); // explicitly disabled via env
        cfg.features.odgm_sidecar = Some(false);
        let cfg = cfg.apply_profile_defaults();
        // Kept as overridden
        assert_eq!(cfg.features.hpa, Some(false));
        assert_eq!(cfg.security_mtls, Some(false));
        assert_eq!(cfg.features.odgm_sidecar, Some(false));
        // Others get profile defaults
        assert_eq!(cfg.features.nfr_enforcement, Some(true));
        assert_eq!(cfg.features.prometheus, Some(true));
        assert_eq!(cfg.features.leader_election, Some(true));
    }
}
