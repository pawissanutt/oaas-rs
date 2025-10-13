use std::process::Command;

fn main() {
    // Get the current timestamp using a simple format
    // We'll use a basic timestamp since we can't easily use chrono in build.rs
    // without adding it as a build dependency
    let now = std::time::SystemTime::now();
    let duration_since_epoch = now
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards");

    // Use seconds since epoch as a simple timestamp
    let build_timestamp = duration_since_epoch.as_secs();
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", build_timestamp);

    // Also get git commit hash if available
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
    {
        if output.status.success() {
            let git_hash =
                String::from_utf8_lossy(&output.stdout).trim().to_string();
            println!("cargo:rustc-env=GIT_HASH={}", git_hash);
        }
    }
}
