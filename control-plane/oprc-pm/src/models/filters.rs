use oprc_models::OPackage;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PackageFilter {
    pub name_pattern: Option<String>,
    pub disabled: Option<bool>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

impl PackageFilter {
    pub fn matches(&self, package: &OPackage) -> bool {
        if let Some(ref pattern) = self.name_pattern {
            if !package.name.contains(pattern) {
                return false;
            }
        }

        if !self.tags.is_empty() {
            let package_tags: HashSet<String> =
                package.metadata.tags.iter().cloned().collect();
            let filter_tags: HashSet<String> =
                self.tags.iter().cloned().collect();
            if filter_tags.intersection(&package_tags).count() == 0 {
                return false;
            }
        }

        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeploymentFilter {
    pub package_name: Option<String>,
    pub class_key: Option<String>,
    pub target_env: Option<String>,
    pub target_cluster: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClassRuntimeFilter {
    pub package_name: Option<String>,
    pub class_key: Option<String>,
    pub environment: Option<String>,
    pub cluster: Option<String>, // Filter by specific cluster
    pub status: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}
