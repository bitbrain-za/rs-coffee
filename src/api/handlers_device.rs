use crate::{app_state::System, config::Config};
use anyhow::Result;
use serde_json::Value;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn echo_post(data: &str, system: System) {
    *system.echo_data.write().unwrap() = data.to_string();
}

pub fn echo_get(system: System) -> Result<String> {
    let data = system.echo_data.read().unwrap().clone();
    Ok(data)
}

pub fn get_config(system: System) -> Result<Value> {
    let config = system.config.read().unwrap();
    Ok(serde_json::to_value(&*config)?)
}

pub fn set_config(data: &str, system: System) -> Result<()> {
    let mut config = system.config.write().unwrap();
    let new_config: Config = serde_json::from_str(data)?;
    config.update(new_config)?;
    Ok(())
}
