#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnforcementMode {
    Off,
    Observe,
    Enforce,
}

impl EnforcementMode {
    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "off" => EnforcementMode::Off,
            "enforce" => EnforcementMode::Enforce,
            _ => EnforcementMode::Observe,
        }
    }
}

impl std::fmt::Display for EnforcementMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnforcementMode::Off => write!(f, "off"),
            EnforcementMode::Observe => write!(f, "observe"),
            EnforcementMode::Enforce => write!(f, "enforce"),
        }
    }
}
