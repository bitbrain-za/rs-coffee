use crate::types::*;
use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum PostInfusion {
    Idle,
    HeatForSteam(Degrees),
    HeatForWater(Degrees),
}

impl PostInfusion {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}
