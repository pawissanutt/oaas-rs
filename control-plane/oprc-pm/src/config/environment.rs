// Environment-specific configuration handling
// This module provides utilities for loading configuration from environment variables

use std::env;

pub fn get_env_var(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

pub fn get_env_var_optional(key: &str) -> Option<String> {
    env::var(key).ok()
}

pub fn get_env_var_parse<T>(key: &str, default: T) -> T
where
    T: std::str::FromStr,
    T::Err: std::fmt::Debug,
{
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

pub fn get_env_var_bool(key: &str, default: bool) -> bool {
    match env::var(key).as_deref() {
        Ok("true") | Ok("1") | Ok("yes") | Ok("on") => true,
        Ok("false") | Ok("0") | Ok("no") | Ok("off") => false,
        _ => default,
    }
}
