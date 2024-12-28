use crate::app_state::ApiState;
use anyhow::Result;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn echo_post(data: &str, system: ApiState) {
    system.lock().unwrap().echo_data = data.to_string();
}

pub fn echo_get(system: ApiState) -> Result<String> {
    let data = system.lock().unwrap().echo_data.clone();
    Ok(data)
}
