#[cfg(test)]
mod tests {
    use crate::*;
    use std::path::PathBuf;
    use ts_rs::{Config, TS};

    #[test]
    fn export_bindings() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let out_dir = PathBuf::from(manifest_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("frontend/oprc-next/src/lib/bindings");

        std::fs::create_dir_all(&out_dir).unwrap();

        // Use export_all which exports dependencies as well
        // We only need to export the root types, dependencies will be handled automatically.
        // But to be safe and ensure everything we want is there, we can export multiple top-level types.

        // Use export_all which requires a Config in ts-rs 12
        let config = Config::default().with_out_dir(&out_dir);

        OPackage::export_all(&config).unwrap();

        // Deployment models might not be fully covered by Package if not linked directly (though they are in OPackage)
        // But let's export it explicitly to be sure.
        OClassDeployment::export_all(&config).unwrap();

        // Status summary might be used standalone?
        DeploymentStatusSummary::export_all(&config).unwrap();

        // Telemetry config
        TelemetryConfig::export_all(&config).unwrap();

        // We probably don't need to export enums/NFRs explicitly if they are used by the above,
        // but it doesn't hurt (just overwrites).

        println!("Exported bindings to {:?}", out_dir);
    }
}
