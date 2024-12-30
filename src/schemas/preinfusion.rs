use super::Error;
use crate::config::Shots as config;
use crate::types::*;
use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PreInfusion {
    pub time: f32,
    pub pressure: Bar,
}

impl PreInfusion {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.pressure < config::MIN_SHOT_PRESSURE_BAR
            || self.pressure > config::MAX_SHOT_PRESSURE_BAR
        {
            return Err(Error::OutOfBounds(format!(
                "Preinfusion pressure must be between {} and {}",
                config::MIN_SHOT_PRESSURE_BAR,
                config::MAX_SHOT_PRESSURE_BAR
            )));
        }

        Ok(())
    }
}
