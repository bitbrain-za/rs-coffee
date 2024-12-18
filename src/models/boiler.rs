use crate::config;
use crate::kv_store::{Error as KvsError, Key, KeyValueStore, Storable, Value};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
pub struct BoilerModelParameters {
    pub thermal_mass: f32,
    pub ambient_transfer_coefficient: f32,
    pub probe_transfer_coefficient: f32,
}

impl Default for BoilerModelParameters {
    fn default() -> Self {
        Self {
            thermal_mass: 1255.8,
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

#[derive(Debug, Copy, Clone)]
pub struct TuningDataPoint {
    time: Instant,
    power: f32,
    ambient_temperature: f32,
    boiler_temperature: f32,
    probe_temperature: f32,
}

pub struct BoilerModel {
    parameters: BoilerModelParameters,

    // manipulated variable
    max_power: u16,
    flow_rate_kg_per_sec: f32,

    // process variables
    probe_temperature: f32,
    boiler_temperature: f32,
    ambient_temperature: f32,

    tuning_data: Vec<TuningDataPoint>,
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

            tuning_data: Vec::new(),
        }
    }

    pub fn set_flow_rate_ml_per_sec(&mut self, flow_rate: f32) {
        self.flow_rate_kg_per_sec = flow_rate / 1000.0;
    }

    pub fn update(&mut self, power: f32, ambient_temperature: f32, dt: Duration) -> (f32, f32) {
        // Heat loss rate due to the flow of water at ambient temperature into the boiler
        let flow_heat_loss = self.flow_rate_kg_per_sec
            * self.parameters.thermal_mass
            * (self.boiler_temperature - ambient_temperature);

        // Boiler temperature change including flow heat loss
        let d_temp_d_time_boiler = (power
            - (self.parameters.ambient_transfer_coefficient
                * (self.boiler_temperature - ambient_temperature))
            - flow_heat_loss)
            / self.parameters.thermal_mass;
        let delta_boiler = d_temp_d_time_boiler * dt.as_secs_f32();

        // Probe temperature change (dependent on boiler temperature)
        let d_temp_d_time_probe = self.parameters.probe_transfer_coefficient
            * (self.boiler_temperature - self.probe_temperature);
        let delta_probe = d_temp_d_time_probe * dt.as_secs_f32();

        // Update states
        self.boiler_temperature += delta_boiler;
        self.probe_temperature += delta_probe;

        self.measure()
    }

    // To determine boiler paramters, we run the boiler for a period and get the curve.
    // We run the simulator for the same power and interval set and curve fit.
    pub fn simulate(
        &mut self,
        power: &[f32],
        ambient_temperature: &[f32],
        interval: &[Duration],
    ) -> Vec<(f32, f32)> {
        assert_eq!(power.len(), ambient_temperature.len());

        power
            .iter()
            .zip(ambient_temperature.iter())
            .zip(interval.iter())
            .map(|(p, interval)| self.update(*p.0, *p.1, *interval))
            .collect()
    }

    pub fn measure(&self) -> (f32, f32) {
        (self.boiler_temperature, self.probe_temperature)
    }

    pub fn capture_tuning_point(&mut self, power: f32) {
        self.tuning_data.push(TuningDataPoint {
            time: Instant::now(),
            power,
            ambient_temperature: self.ambient_temperature,
            boiler_temperature: self.boiler_temperature,
            probe_temperature: self.probe_temperature,
        });
    }

    pub fn start_tuning(&mut self) {
        self.tuning_data.clear();
    }

    pub fn stop_tuning(&mut self) -> Vec<TuningDataPoint> {
        let data = self.tuning_data.clone();
        self.tuning_data.clear();

        data
    }
}
