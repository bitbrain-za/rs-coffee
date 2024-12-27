use super::{Temperature, Watts};
use crate::config;
use crate::kv_store::{Error as KvsError, Key, KeyValueStore, Storable, Value};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq)]
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

impl BoilerModelParameters {
    const THERMAL_CAPACITY_WATER: f32 = 4186.0;

    pub fn system_model(
        self,
        power: Watts,
        modeled_boiler_temperature: Temperature,
        probe_temperature: Temperature,
        ambient_temperature: Temperature,
        flow_rate_kg_per_sec: f32,
        dt: Duration,
    ) -> (Temperature, Temperature) {
        // Heat loss rate due to the flow of water at ambient temperature into the boiler
        let flow_heat_loss = flow_rate_kg_per_sec
            * Self::THERMAL_CAPACITY_WATER
            * (modeled_boiler_temperature - ambient_temperature);

        // Boiler temperature change including flow heat loss
        let d_temp_d_time_boiler = (power
            - (self.ambient_transfer_coefficient
                * (modeled_boiler_temperature - ambient_temperature))
            - flow_heat_loss)
            / self.thermal_mass;
        let delta_boiler = d_temp_d_time_boiler * dt.as_secs_f32();

        // Probe temperature change (dependent on boiler temperature)
        let d_temp_d_time_probe =
            self.probe_responsiveness * (modeled_boiler_temperature - probe_temperature);
        let delta_probe = d_temp_d_time_probe * dt.as_secs_f32();

        (delta_boiler, delta_probe)
    }
}

#[derive(Default)]
pub struct BoilerModel {
    pub max_power: Watts,
    pub parameters: BoilerModelParameters,

    // manipulated variable
    flow_rate_kg_per_sec: f32,

    // process variables
    pub probe_temperature: Temperature,
    boiler_temperature: Temperature,
    pub ambient_temperature: Temperature,

    power: Watts,
    smoothing_factor: f32,
}

impl BoilerModel {
    pub fn new(initial_temperature: Option<Temperature>) -> Self {
        Self {
            max_power: config::BOILER_POWER,
            parameters: BoilerModelParameters::load_or_default(),

            flow_rate_kg_per_sec: 0.0,

            probe_temperature: initial_temperature.unwrap_or(config::INITIAL_TEMPERATURE),
            boiler_temperature: initial_temperature.unwrap_or(config::INITIAL_TEMPERATURE),
            ambient_temperature: initial_temperature.unwrap_or(config::INITIAL_TEMPERATURE),

            power: 0.0,
            smoothing_factor: config::MPC_SMOOTHING_FACTOR,
        }
    }

    pub fn update_parameters(
        &mut self,
        parameters: BoilerModelParameters,
        probe_temperature: Temperature,
        boiler_temperature: Temperature,
        ambient_temperature: Temperature,
    ) {
        self.parameters = parameters;
        // self.parameters.save().unwrap();

        self.ambient_temperature = ambient_temperature;
        self.boiler_temperature = boiler_temperature;
        self.probe_temperature = probe_temperature;
    }

    pub fn set_flow_rate_ml_per_sec(&mut self, flow_rate: f32) {
        self.flow_rate_kg_per_sec = flow_rate / 1000.0;
    }

    #[cfg(feature = "simulate")]
    pub fn get_noisy_probe(&self) -> Temperature {
        use rand::prelude::*;
        let distribution = rand_distr::Normal::new(0.0, 1.0).unwrap();
        let noise: f32 = distribution.sample(&mut thread_rng()) / 10.0;
        self.probe_temperature + noise
    }

    #[cfg(feature = "simulate")]
    pub fn get_actual_temperature(&self) -> Temperature {
        self.boiler_temperature
    }

    pub fn get_duty_cycle(&self) -> f32 {
        self.power / self.max_power
    }

    pub fn update(&mut self, power: Watts, dt: Duration) -> (Temperature, Temperature) {
        let (delta_boiler_temperature, delta_probe_temperature) = self.parameters.system_model(
            power,
            self.boiler_temperature,
            self.probe_temperature,
            self.ambient_temperature,
            self.flow_rate_kg_per_sec,
            dt,
        );

        // Update states
        self.boiler_temperature += delta_boiler_temperature;
        self.probe_temperature += delta_probe_temperature;

        (self.boiler_temperature, self.probe_temperature)
    }

    pub fn control(
        &mut self,
        current_probe_temperature: Temperature,
        ambient_temperature: Temperature,
        setpoint: Temperature,
        control_loop_time: Duration,
    ) -> Watts {
        let (delta_boiler_temperature, _) = self.parameters.system_model(
            self.power,
            self.boiler_temperature,
            current_probe_temperature,
            ambient_temperature,
            self.flow_rate_kg_per_sec,
            control_loop_time,
        );

        let correction =
            self.smoothing_factor * (current_probe_temperature - self.probe_temperature);

        self.boiler_temperature += correction;
        self.probe_temperature += correction;

        let boiler_predicted_temperature = self.boiler_temperature + delta_boiler_temperature;

        let mut power = (setpoint - boiler_predicted_temperature) * self.parameters.thermal_mass
            / (2.0 * control_loop_time.as_secs_f32());
        power -= (ambient_temperature - boiler_predicted_temperature)
            * self.parameters.ambient_transfer_coefficient;

        if power < 0.0 {
            power = 0.0;
        } else if power > self.max_power {
            power = self.max_power;
        }

        self.power = power;
        self.power
    }
}
