use crate::config;
use crate::kv_store::{Error as KvsError, Key, KeyValueStore, Storable, Value};

use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
pub struct BoilerModelParameters {
    pub boiler_thermal_mass: f32,
    pub ambient_transfer_coefficient: f32,
    pub probe_transfer_coefficient: f32,
}

impl Default for BoilerModelParameters {
    fn default() -> Self {
        Self {
            boiler_thermal_mass: 1255.8,
            ambient_transfer_coefficient: 0.8,
            probe_transfer_coefficient: 0.0125,
        }
    }
}

impl From<&BoilerModelParameters> for Value {
    fn from(params: &BoilerModelParameters) -> Self {
        Value::BoilerModelParameters(*params)
    }
}

impl Storable for BoilerModelParameters {
    fn load_or_default() -> Self {
        let kvs = match KeyValueStore::new_blocking(std::time::Duration::from_millis(1000)) {
            Ok(kvs) => kvs,
            Err(e) => {
                log::error!("Failed to create key value store: {:?}", e);
                return Self::default();
            }
        };
        match kvs.get(Key::BoilerModelParameters) {
            Value::BoilerModelParameters(calibration) => calibration,
            _ => Self::default(),
        }
    }

    fn save(&self) -> Result<(), KvsError> {
        let mut kvs = KeyValueStore::new_blocking(std::time::Duration::from_millis(1000))?;
        kvs.set(self.into())
    }
}

pub struct BoilerModel {
    max_power: u16,
    boiler_parameters: BoilerModelParameters,
    flow_rate: f32,
}

impl BoilerModel {
    pub fn new() -> Self {
        Self {
            max_power: config::BOILER_POWER,
            boiler_parameters: BoilerModelParameters::load_or_default(),

            flow_rate: 0.0,
        }
    }
}
