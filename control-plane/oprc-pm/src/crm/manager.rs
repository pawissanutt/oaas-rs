use crate::{
    config::CrmManagerConfig,
    crm::CrmClient,
    errors::CrmError,
    models::{ClassRuntime, ClassRuntimeFilter, ClusterHealth},
};

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

pub struct CrmManager {
    clients: HashMap<String, Arc<CrmClient>>,
    default_client: Option<Arc<CrmClient>>,
    health_cache: RwLock<HashMap<String, (ClusterHealth, Instant)>>,
    health_cache_ttl: Duration,
}

impl CrmManager {
    pub fn new(config: CrmManagerConfig) -> Result<Self, CrmError> {
        let mut clients = HashMap::new();
        let mut default_client = None;

        for (cluster_name, crm_config) in config.clusters.iter() {
            let client = Arc::new(CrmClient::new(
                cluster_name.clone(),
                crm_config.clone(),
            )?);

            if config.default_cluster.as_ref() == Some(cluster_name) {
                default_client = Some(client.clone());
            }

            clients.insert(cluster_name.clone(), client);
        }

        // If no default was set but we have clusters, use the first one
        if default_client.is_none() && !clients.is_empty() {
            default_client = clients.values().next().cloned();
        }

        Ok(Self {
            clients,
            default_client,
            health_cache: RwLock::new(HashMap::new()),
            health_cache_ttl: Duration::from_secs(
                config.health_cache_ttl_seconds,
            ),
        })
    }

    pub async fn get_client(
        &self,
        cluster_name: &str,
    ) -> Result<Arc<CrmClient>, CrmError> {
        self.clients
            .get(cluster_name)
            .cloned()
            .ok_or_else(|| CrmError::ClusterNotFound(cluster_name.to_string()))
    }

    pub async fn get_default_client(&self) -> Result<Arc<CrmClient>, CrmError> {
        self.default_client
            .clone()
            .ok_or(CrmError::NoDefaultCluster)
    }

    pub async fn list_clusters(&self) -> Vec<String> {
        self.clients.keys().cloned().collect()
    }

    pub async fn get_cluster_health(
        &self,
        cluster_name: &str,
    ) -> Result<ClusterHealth, CrmError> {
        // Fast path: cached health
        if let Some(cached) = {
            let map = self.health_cache.read().await;
            map.get(cluster_name).cloned()
        } {
            if cached.1.elapsed() < self.health_cache_ttl {
                debug!(cluster=%cluster_name, "Serving health from cache");
                return Ok(cached.0);
            }
        }

        let client = self.get_client(cluster_name).await?;
        let health = client.health_check().await?;
        {
            let mut map = self.health_cache.write().await;
            map.insert(
                cluster_name.to_string(),
                (health.clone(), Instant::now()),
            );
        }
        Ok(health)
    }

    pub async fn get_all_class_runtimes(
        &self,
        filter: ClassRuntimeFilter,
    ) -> Result<Vec<ClassRuntime>, CrmError> {
        info!(
            "Getting deployment records from all clusters with filter: {:?}",
            filter
        );

        let mut all_records = Vec::new();

        // Query all clusters or specific cluster if specified in filter
        let clusters_to_query = if let Some(cluster) = &filter.cluster {
            vec![cluster.clone()]
        } else {
            self.list_clusters().await
        };

        for cluster_name in &clusters_to_query {
            if let Ok(client) = self.get_client(&cluster_name).await {
                match client.list_class_runtimes(filter.clone()).await {
                    Ok(mut records) => {
                        // Tag records with their cluster information
                        for record in &mut records {
                            record.cluster_name = Some(cluster_name.clone());
                        }
                        all_records.extend(records);
                    }
                    Err(e) => {
                        warn!(
                            "Failed to fetch records from cluster {}: {}",
                            cluster_name, e
                        );
                        // Continue with other clusters
                    }
                }
            } else {
                warn!("Could not get client for cluster: {}", cluster_name);
            }
        }

        // Apply global filtering and pagination
        if let Some(limit) = filter.limit {
            all_records.truncate(limit);
        }

        info!(
            "Retrieved {} deployment records from {} clusters",
            all_records.len(),
            clusters_to_query.len()
        );
        Ok(all_records)
    }

    pub async fn check_all_cluster_health(
        &self,
    ) -> HashMap<String, Result<ClusterHealth, CrmError>> {
        info!("Checking health of all clusters");

        let mut health_results = HashMap::new();

        for cluster_name in self.list_clusters().await {
            let health_result = self.get_cluster_health(&cluster_name).await;
            health_results.insert(cluster_name, health_result);
        }

        health_results
    }

    pub async fn get_healthy_clusters(&self) -> Vec<String> {
        let health_results = self.check_all_cluster_health().await;
        let mut healthy_clusters = Vec::new();

        for (cluster_name, health_result) in health_results {
            match health_result {
                Ok(health) if health.status == "Healthy" => {
                    healthy_clusters.push(cluster_name);
                }
                Ok(health) => {
                    debug!(cluster=%cluster_name, status=%health.status, "Cluster not healthy");
                }
                Err(e) => {
                    warn!(cluster=%cluster_name, error=%e, "Health check failed");
                }
            }
        }

        healthy_clusters
    }

    pub async fn select_deployment_clusters(
        &self,
        target_clusters: &[String],
    ) -> Result<Vec<String>, CrmError> {
        info!("Selecting deployment envs from: {:?}", target_clusters);

        if target_clusters.is_empty() {
            // Use all healthy clusters if none specified
            let healthy = self.get_healthy_clusters().await;
            if healthy.is_empty() {
                return Err(CrmError::ServiceUnavailable);
            }
            return Ok(healthy);
        }

        // Validate that all target clusters are available & prefer healthy ones first
        let mut available_clusters = Vec::new();
        let mut degraded_or_unhealthy = Vec::new();
        for cluster_name in target_clusters {
            if !self.clients.contains_key(cluster_name) {
                return Err(CrmError::ClusterNotFound(cluster_name.clone()));
            }
            match self.get_cluster_health(cluster_name).await {
                Ok(health) if health.status == "Healthy" => {
                    available_clusters.push(cluster_name.clone());
                }
                Ok(health) => {
                    warn!(cluster=%cluster_name, status=%health.status, "Including degraded cluster");
                    degraded_or_unhealthy.push(cluster_name.clone());
                }
                Err(e) => {
                    warn!(cluster=%cluster_name, error=%e, "Including unreachable cluster");
                    degraded_or_unhealthy.push(cluster_name.clone());
                }
            }
        }
        // Append degraded/unhealthy after healthy to guide caller ordering
        available_clusters.extend(degraded_or_unhealthy);
        Ok(available_clusters)
    }
}
