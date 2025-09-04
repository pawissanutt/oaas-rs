use std::env;
use std::panic::{self, AssertUnwindSafe};

use serial_test::serial;

use oprc_pm::AppConfig;

fn clear_crm_env() {
    let keys: Vec<String> = env::vars()
        .filter_map(|(k, _)| if k.starts_with("CRM_") { Some(k) } else { None })
        .collect();
    for k in keys {
        unsafe {
            env::remove_var(k);
        }
    }
}

fn set_env(k: &str, v: &str) {
    unsafe {
        env::set_var(k, v);
    }
}

#[test]
#[serial]
fn single_default_cluster_backward_compat() {
    clear_crm_env();
    set_env("CRM_DEFAULT_URL", "http://localhost:8088");

    let cfg = AppConfig::load_from_env().expect("load env");
    let crm = cfg.crm();

    assert_eq!(crm.clusters.len(), 1);
    let c = crm.clusters.get("default").expect("default cluster");
    assert_eq!(c.url, "http://localhost:8088");
    assert_eq!(crm.default_cluster.as_deref(), Some("default"));
}

#[test]
#[serial]
fn env_list_multiple_clusters_with_default_and_tls() {
    clear_crm_env();
    set_env("CRM_CLUSTERS", "prod,staging,edge-eu");
    set_env("CRM_DEFAULT_CLUSTER", "prod");

    // prod with TLS enabled
    set_env("CRM_CLUSTER_PROD_URL", "https://crm-prod:8443");
    set_env("CRM_CLUSTER_PROD_TLS_ENABLED", "true");
    set_env("CRM_CLUSTER_PROD_TLS_CA_CERT_PATH", "/certs/ca.pem");
    set_env("CRM_CLUSTER_PROD_RETRY_ATTEMPTS", "7");

    // staging with explicit retry/timeouts
    set_env("CRM_CLUSTER_STAGING_URL", "http://crm-stg:8088");
    set_env("CRM_CLUSTER_STAGING_RETRY_ATTEMPTS", "5");
    set_env("CRM_CLUSTER_STAGING_TIMEOUT", "45");

    // edge-eu uses sanitized suffix
    set_env("CRM_CLUSTER_EDGE_EU_URL", "http://crm-eu:8088");

    let cfg = AppConfig::load_from_env().expect("load env");
    let crm = cfg.crm();

    assert_eq!(crm.clusters.len(), 3);
    assert_eq!(crm.default_cluster.as_deref(), Some("prod"));

    let prod = crm.clusters.get("prod").unwrap();
    assert_eq!(prod.url, "https://crm-prod:8443");
    assert_eq!(prod.retry_attempts, 7);
    assert!(prod.tls.is_some());
    let tls = prod.tls.as_ref().unwrap();
    assert_eq!(tls.ca_cert.as_deref(), Some("/certs/ca.pem"));

    let staging = crm.clusters.get("staging").unwrap();
    assert_eq!(staging.url, "http://crm-stg:8088");
    assert_eq!(staging.retry_attempts, 5);
    assert_eq!(staging.timeout, Some(45));

    let edge = crm.clusters.get("edge-eu").unwrap();
    assert_eq!(edge.url, "http://crm-eu:8088");
}

#[test]
#[serial]
fn json_object_precedence_over_env_list() {
    clear_crm_env();
    // An env list that should be ignored due to JSON precedence
    set_env("CRM_CLUSTERS", "prod");
    set_env("CRM_CLUSTER_PROD_URL", "http://ignored:8088");

    // JSON object form
    set_env(
        "CRM_CLUSTERS_JSON",
        "{\"json\":{\"url\":\"http://json:1\"}}",
    );
    set_env("CRM_DEFAULT_CLUSTER", "json");

    let cfg = AppConfig::load_from_env().expect("load env");
    let crm = cfg.crm();

    assert_eq!(crm.clusters.len(), 1);
    assert!(crm.clusters.get("json").is_some());
    assert_eq!(crm.clusters.get("json").unwrap().url, "http://json:1");
    assert_eq!(crm.default_cluster.as_deref(), Some("json"));
}

#[test]
#[serial]
fn json_array_requires_name_panics() {
    clear_crm_env();
    set_env("CRM_CLUSTERS_JSON", "[{\"url\":\"http://no-name\"}]");

    let cfg = AppConfig::load_from_env().expect("load env");
    let result = panic::catch_unwind(AssertUnwindSafe(|| cfg.crm()));
    assert!(
        result.is_err(),
        "expected panic due to missing 'name' in array item"
    );
}

#[test]
#[serial]
fn env_list_invalid_name_panics() {
    clear_crm_env();
    set_env("CRM_CLUSTERS", "bad$name");

    let cfg = AppConfig::load_from_env().expect("load env");
    let result = panic::catch_unwind(AssertUnwindSafe(|| cfg.crm()));
    assert!(
        result.is_err(),
        "expected panic due to invalid cluster name"
    );
}

#[test]
#[serial]
fn default_cluster_falls_back_to_named_default() {
    clear_crm_env();
    set_env("CRM_CLUSTERS", "prod,default,staging");
    set_env("CRM_CLUSTER_PROD_URL", "http://p:1");
    set_env("CRM_CLUSTER_DEFAULT_URL", "http://d:1");
    set_env("CRM_CLUSTER_STAGING_URL", "http://s:1");

    let cfg = AppConfig::load_from_env().expect("load env");
    let crm = cfg.crm();

    assert_eq!(crm.default_cluster.as_deref(), Some("default"));
}

#[test]
#[serial]
fn tls_auto_when_paths_present_without_enabled_flag() {
    clear_crm_env();
    set_env("CRM_CLUSTERS", "staging");
    set_env("CRM_CLUSTER_STAGING_URL", "http://crm-stg:8088");
    // No TLS_ENABLED flag, but paths provided
    set_env("CRM_CLUSTER_STAGING_TLS_CA_CERT_PATH", "/ca.pem");

    let cfg = AppConfig::load_from_env().expect("load env");
    let crm = cfg.crm();

    let stg = crm.clusters.get("staging").unwrap();
    assert!(stg.tls.is_some());
}
