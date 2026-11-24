//! Load one resolved legacy plan/CAS snapshot; never consult process configuration.

use crate::application::ports::SessionSettings;
use isolock_cas::InMemoryCas;
use isolock_plan::AirlockPlan;
use std::fs;

pub fn load(config: &SessionSettings) -> Result<(AirlockPlan, InMemoryCas), String> {
    let plan: AirlockPlan = match fs::read(&config.plan) {
        Ok(contents) => serde_json::from_slice(&contents)
            .map_err(|error| format!("invalid plan {}: {error}", config.plan.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !config.require_plan => {
            AirlockPlan::new()
        }
        Err(error) => {
            return Err(format!(
                "cannot read plan {}: {error}",
                config.plan.display()
            ))
        }
    };
    let mut cas = InMemoryCas::new();
    for artifact in plan.artifacts() {
        let path = config.cas_dir.join(artifact.digest.sharded_path());
        match fs::read(&path) {
            Ok(bytes) => cas
                .insert(artifact.digest.clone(), bytes)
                .map_err(|error| format!("invalid CAS blob {}: {error}", path.display()))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("cannot read CAS blob {}: {error}", path.display())),
        }
    }
    Ok((plan, cas))
}
