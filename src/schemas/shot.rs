use super::Error;
use crate::config::Shots as config;
use crate::types::*;
use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Shot {
    pub weight: Option<Grams>,
    pub time: Option<f32>,
    pub profile: Vec<Profile>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct Profile {
    pub degrees: Degrees,
    pub pressure: Bar,
    pub percentage: u8,
}

pub struct ShotBuilder {
    pub weight: Option<Grams>,
    pub time: Option<f32>,
    pub profile: Vec<Profile>,
}

impl Shot {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.weight.is_none() && self.time.is_none() {
            return Err(Error::MissingOutputSpecifier);
        }
        if self.weight.is_some() && self.time.is_some() {
            return Err(Error::InvalidProfile(
                "Cannot specify both weight and time".to_string(),
            ));
        }
        if self.profile.is_empty() {
            return Err(Error::MissingProfile);
        }
        for profile in &self.profile {
            profile.validate()?;
        }
        let percentage_sum: u8 = self.profile.iter().map(|p| p.percentage).sum();
        if percentage_sum != 100 {
            return Err(Error::InvalidProfile(format!(
                "Profile percentages do not sum to 100: {}",
                percentage_sum
            )));
        }

        Ok(())
    }
}

impl std::fmt::Display for Shot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Err(e) = self.validate() {
            return write!(f, "Invalid shot: {}", e);
        }
        let output = if let Some(weight) = self.weight {
            format!("{}g", weight)
        } else if let Some(time) = self.time {
            format!("{}s", time)
        } else {
            unreachable!()
        };
        let mut profile = String::new();
        for p in &self.profile {
            profile.push_str(&format!(
                "\n{}C, {}bar for {}%",
                p.degrees, p.pressure, p.percentage
            ));
        }

        write!(f, "{} shot with {}", output, profile)
    }
}

impl Profile {
    pub fn new(degrees: Degrees, pressure: Bar, percentage: u8) -> Self {
        Profile {
            degrees,
            pressure,
            percentage,
        }
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.degrees < config::MIN_SHOT_TEMPERATURE
            || self.degrees > config::MAX_SHOT_TEMPERATURE
        {
            return Err(Error::OutOfBounds(format!(
                "Invalid degrees: {}",
                self.degrees
            )));
        }
        if self.pressure < config::MIN_SHOT_PRESSURE_BAR
            || self.pressure > config::MAX_SHOT_PRESSURE_BAR
        {
            return Err(Error::OutOfBounds(format!(
                "Invalid pressure: {}",
                self.pressure
            )));
        }
        if self.percentage > 100 {
            return Err(Error::InvalidProfile(format!(
                "Invalid percentage: {}",
                self.percentage
            )));
        }

        Ok(())
    }
}

impl ShotBuilder {
    pub fn new() -> Self {
        ShotBuilder {
            weight: None,
            time: None,
            profile: Vec::new(),
        }
    }

    pub fn by_weight(mut self, weight: Grams) -> Self {
        self.time = None;
        self.weight = Some(weight);
        self
    }

    pub fn by_time(mut self, time: f32) -> Self {
        self.weight = None;
        self.time = Some(time);
        self
    }

    pub fn add_profile(mut self, profile: Profile) -> Self {
        self.profile.push(profile);
        self
    }

    pub fn build(self) -> Result<Shot, Error> {
        let shot = Shot {
            weight: self.weight,
            time: self.time,
            profile: self.profile,
        };
        shot.validate()?;
        Ok(shot)
    }
}
