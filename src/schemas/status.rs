use crate::app_state::System;
use crate::types::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Device {
    temperature: Temperature,
    pressure: Bar,
    weight: Grams,
    ambient: Temperature,
    power: Watts,
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

    pub fn new(_system: &System) -> Self {
        StatusReport {
            status: "healthy".to_string(),
            message: None,
            device: Device {
                temperature: 0.0,
                pressure: 0.0,
                weight: 0.0,
                ambient: 0.0,
                power: 0.0,
            },
            operation: Operation {
                state: "idle".to_string(),
                attributes: Some(serde_json::json!({ "target": 60.0 })),
            },
        }
    }
}
