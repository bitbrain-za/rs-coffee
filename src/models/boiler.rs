use crate::config;
use crate::kv_store::{Error as KvsError, Key, KeyValueStore, Storable, Value};
use std::time::Duration;

use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
pub struct BoilerModelParameters {
    pub thermal_mass: f32,
    pub ambient_transfer_coefficient: f32,
    pub probe_responsiveness: f32,
}

impl Default for BoilerModelParameters {
    fn default() -> Self {
        Self {
            thermal_mass: 1255.8,
            ambient_transfer_coefficient: 0.0664,
            probe_responsiveness: 0.1,
        }
    }
}

impl std::fmt::Display for BoilerModelParameters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Thermal Mass: {}\nAmbient Transfer Coefficient: {}\nProbe Responsiveness: {}\n",
            self.thermal_mass, self.ambient_transfer_coefficient, self.probe_responsiveness
        )
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

#[derive(Default)]
pub struct BoilerModel {
    pub parameters: BoilerModelParameters,
    pub max_power: f32,

    // manipulated variable
    flow_rate_kg_per_sec: f32,

    // process variables
    pub probe_temperature: f32,
    boiler_temperature: f32,
    pub ambient_temperature: f32,
}

impl BoilerModel {
    pub fn new(initial_temperature: Option<f32>) -> Self {
        Self {
            max_power: config::BOILER_POWER,
            parameters: BoilerModelParameters::load_or_default(),

            flow_rate_kg_per_sec: 0.0,

            probe_temperature: initial_temperature.unwrap_or(config::INITIAL_TEMPERATURE),
            boiler_temperature: initial_temperature.unwrap_or(config::INITIAL_TEMPERATURE),
            ambient_temperature: initial_temperature.unwrap_or(config::INITIAL_TEMPERATURE),
        }
    }

    pub fn set_flow_rate_ml_per_sec(&mut self, flow_rate: f32) {
        self.flow_rate_kg_per_sec = flow_rate / 1000.0;
    }

    #[cfg(feature = "simulate")]
    pub fn get_noisy_probe(&self) -> f32 {
        use rand::prelude::*;
        let distribution = rand_distr::Normal::new(0.0, 1.0).unwrap();
        let noise: f32 = distribution.sample(&mut thread_rng()) / 10.0;
        self.probe_temperature + noise
    }

    #[cfg(feature = "simulate")]
    pub fn get_actual_temperature(&self) -> f32 {
        self.boiler_temperature
    }
    pub fn update(&mut self, power: f32, dt: Duration) -> (f32, f32) {
        // Heat loss rate due to the flow of water at ambient temperature into the boiler
        let flow_heat_loss = self.flow_rate_kg_per_sec
            * self.parameters.thermal_mass
            * (self.boiler_temperature - self.ambient_temperature);

        // Boiler temperature change including flow heat loss
        let d_temp_d_time_boiler = (power
            - (self.parameters.ambient_transfer_coefficient
                * (self.boiler_temperature - self.ambient_temperature))
            - flow_heat_loss)
            / self.parameters.thermal_mass;
        let delta_boiler = d_temp_d_time_boiler * dt.as_secs_f32();

        // Probe temperature change (dependent on boiler temperature)
        let d_temp_d_time_probe = self.parameters.probe_responsiveness
            * (self.boiler_temperature - self.probe_temperature);
        let delta_probe = d_temp_d_time_probe * dt.as_secs_f32();

        // Update states
        self.boiler_temperature += delta_boiler;
        self.probe_temperature += delta_probe;

        (self.boiler_temperature, self.probe_temperature)
    }
}
