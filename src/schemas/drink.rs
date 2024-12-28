use super::{postinfusion::PostInfusion, preinfusion::PreInfusion, shot::Shot, Error};
use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Drink {
    pub name: Option<String>,
    pub preinfusion: Option<PreInfusion>,
    pub shot: Shot,
    pub postinfusion: Option<PostInfusion>,
}

impl Drink {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn validate(&self) -> Result<(), Error> {
        self.shot.validate()?;
        if let Some(preinfusion) = &self.preinfusion {
            preinfusion.validate()?;
        }

        Ok(())
    }
}
