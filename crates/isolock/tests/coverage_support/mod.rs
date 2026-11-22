//! Keep instrumented child processes visible while isolating application configuration.

use std::process::Command;

pub fn preserve_profile(command: &mut Command) {
    // cargo-llvm-cov supplies a per-process filename pattern. Without this, env_clear()
    // sends the child's profile into its temporary project, which is deleted after the test.
    if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
}
