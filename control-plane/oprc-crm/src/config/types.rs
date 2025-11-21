use envconfig::Envconfig;

#[derive(Envconfig, Clone, Debug)]
pub struct CrmConfig {
    #[envconfig(from = "OPRC_CRM_PROFILE", default = "dev")]
    pub profile: String,

    #[envconfig(from = "HTTP_PORT", default = "8088")]
    pub http_port: u16,

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

    #[envconfig(nested)]
    pub templates: TemplatesConfig,

    #[envconfig(nested)]
    pub prometheus: PromConfig,

    #[envconfig(nested)]
    pub otel: OtelConfig,

    /// Analyzer loop interval in seconds.
    /// Env: OPRC_CRM_ANALYZER_INTERVAL_SECS
    #[envconfig(from = "OPRC_CRM_ANALYZER_INTERVAL_SECS", default = "60")]
    pub analyzer_interval_secs: u64,
}

#[derive(Envconfig, Clone, Debug)]
pub struct OtelConfig {
    #[envconfig(from = "OPRC_CRM_OTEL_ENABLED", default = "false")]
    pub enabled: bool,
    #[envconfig(
        from = "OPRC_CRM_OTEL_ENDPOINT",
        default = "http://otel-collector:4317"
    )]
    pub endpoint: String,
}

#[derive(Envconfig, Clone, Debug, Default)]
pub struct FeaturesConfig {
    /// If Some, env explicitly set; otherwise, profile defaults apply
    #[envconfig(from = "OPRC_CRM_FEATURES_NFR_ENFORCEMENT")]
    pub nfr_enforcement: Option<bool>,
    #[envconfig(from = "OPRC_CRM_FEATURES_HPA")]
    pub hpa: Option<bool>,
    #[envconfig(from = "OPRC_CRM_FEATURES_KNATIVE", default = "true")]
    pub knative: bool,
    #[envconfig(from = "OPRC_CRM_FEATURES_PROMETHEUS", default = "false")]
    pub prometheus: bool,
    #[envconfig(from = "OPRC_CRM_FEATURES_LEADER_ELECTION", default = "false")]
    pub leader_election: bool,
    #[envconfig(from = "OPRC_CRM_FEATURES_ODGM", default = "true")]
    pub odgm_sidecar: bool,
    /// Enable FSM-based reconcile (template descriptors + readiness rules)
    /// Env: OPRC_CRM_FEATURES_FSM
    #[envconfig(from = "OPRC_CRM_FEATURES_FSM", default = "false")]
    pub fsm: bool,
}

#[derive(Envconfig, Clone, Debug)]
pub struct EnforcementConfig {
    #[envconfig(from = "OPRC_CRM_ENFORCEMENT_COOLDOWN_SECS", default = "120")]
    pub cooldown_secs: u64,
    /// Require recommendations to be stable for this many seconds before applying
    /// Env: OPRC_CRM_ENFORCEMENT_STABILITY_SECS
    #[envconfig(from = "OPRC_CRM_ENFORCEMENT_STABILITY_SECS", default = "180")]
    pub stability_secs: u64,
    #[envconfig(
        from = "OPRC_CRM_ENFORCEMENT_MAX_REPLICA_DELTA",
        default = "30"
    )]
    pub max_replica_delta_pct: u8,
    #[envconfig(from = "OPRC_CRM_LIMITS_MAX_REPLICAS", default = "20")]
    pub max_replicas: u32,
    /// Per-pod CPU request used in replicas_min formula when needed
    /// Env: OPRC_CRM_REQ_CPU_PER_POD_M
    #[envconfig(from = "OPRC_CRM_REQ_CPU_PER_POD_M", default = "500")]
    pub req_cpu_per_pod_m: u32,
}

impl CrmConfig {
    /// Apply profile → defaults mapping, while respecting explicit env overrides.
    ///
    /// Rules:
    /// - dev: nfr_enforcement=false, hpa=false, knative=false, prometheus=false, leader_election=false, mTLS=false
    /// - edge: nfr_enforcement=false, hpa=true,  knative=false, prometheus=true,  leader_election=true,  mTLS=false
    /// - full: nfr_enforcement=true,  hpa=true,  knative=false, prometheus=true,  leader_election=true,  mTLS=true
    pub fn apply_profile_defaults(mut self) -> Self {
        let (def_nfr, def_hpa, def_mtls) = match self.profile.as_str() {
                "edge" => (false, true,  false),
                "full" | "prod" | "production" => {
                    (true, true,  true)
                }
                _ /* dev */ => (false, false,  false ),
            };

        // Only set when not explicitly provided via env
        if self.features.nfr_enforcement.is_none() {
            self.features.nfr_enforcement = Some(def_nfr);
        }
        if self.features.hpa.is_none() {
            self.features.hpa = Some(def_hpa);
        }
        if self.security_mtls.is_none() {
            self.security_mtls = Some(def_mtls);
        }

        self
    }
}

/// Prometheus-related environment configuration (operator-only metrics)
#[derive(Envconfig, Clone, Debug, Default)]
pub struct PromConfig {
    /// Base URL for Prometheus HTTP API (e.g., http://prometheus-k8s.monitoring.svc:9090)
    /// Env: OPRC_CRM_PROM_URL
    #[envconfig(from = "OPRC_CRM_PROM_URL")]
    pub url: Option<String>,

    /// Comma-separated key=value labels to add on ServiceMonitor/PodMonitor
    /// so Prometheus Operator selects them (e.g., "release=prometheus").
    /// Env: OPRC_CRM_PROM_MATCH_LABELS
    #[envconfig(from = "OPRC_CRM_PROM_MATCH_LABELS")]
    pub match_labels: Option<String>,

    /// Controls whether to manage ServiceMonitor, PodMonitor, or pick based on runtime (Knative → pod).
    /// Env: OPRC_CRM_PROM_SCRAPE_KIND (service | pod | auto)
    #[envconfig(from = "OPRC_CRM_PROM_SCRAPE_KIND")]
    pub scrape_kind: Option<String>,

    /// Query timeout (seconds) for observe-only computations.
    /// Env: OPRC_CRM_PROM_QUERY_TIMEOUT_SECS
    #[envconfig(from = "OPRC_CRM_PROM_QUERY_TIMEOUT_SECS", default = "5")]
    pub query_timeout_secs: u64,

    /// Range and step for range queries (not yet used in Analyzer)
    /// Env: OPRC_CRM_PROM_RANGE (e.g., "10m"), OPRC_CRM_PROM_STEP (e.g., "30s")
    #[envconfig(from = "OPRC_CRM_PROM_RANGE", default = "10m")]
    pub range: String,
    #[envconfig(from = "OPRC_CRM_PROM_STEP", default = "30s")]
    pub step: String,
}

/// Templates-related configuration (render-time knobs)
#[derive(Envconfig, Clone, Debug, Default)]
pub struct TemplatesConfig {
    /// Optional override for the ODGM sidecar container image used by templates.
    /// When set, all templates will use this image for ODGM instead of their built-in defaults.
    /// Env: OPRC_CRM_TEMPLATES_ODGM_IMAGE
    #[envconfig(from = "OPRC_CRM_TEMPLATES_ODGM_IMAGE")]
    pub odgm_img_override: Option<String>,
    /// Optional override for the ODGM sidecar imagePullPolicy (Always|IfNotPresent|Never).
    /// When unset, Kubernetes defaults apply (e.g. Always for :latest tags, IfNotPresent otherwise).
    /// Env: OPRC_CRM_TEMPLATES_ODGM_PULL_POLICY
    #[envconfig(from = "OPRC_CRM_TEMPLATES_ODGM_PULL_POLICY")]
    pub odgm_pull_policy_override: Option<String>,
}
