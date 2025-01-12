use crate::types::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Switches {
    pub brew: bool,
    pub water: bool,
    pub steam: bool,
}
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Device {
    pub temperature: Temperature,
    pub pressure: Bar,
    pub weight: Grams,
    pub ambient: Temperature,
    pub power: Watts,
    pub level: Millimeters,
    pub switches: Switches,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Operation {
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StatusReport {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub device: Device,
    pub operation: Operation,
}

impl StatusReport {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}
