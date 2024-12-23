use crate::models::boiler::{BoilerModel, BoilerModelParameters};
use std::time::{Duration, Instant};

const HEATER_MAX_POWER: f32 = 1000.0;
const TRANSFER_TEST_HEATER_POWER: f32 = HEATER_MAX_POWER * 0.5;
const TARGET_TEMPERATURE: f32 = 94.0;

#[derive(Debug, Default)]
struct DifferentialData {
    rate: f32,
    temperature: f32,
    time: Option<Instant>,
}

#[derive(Debug)]
pub enum Error {
    TemperatureNotStable,
    TemperatureOutOfBounds(String),
    UnableToPerformTest(String),
    InsufficientData(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Error::TemperatureNotStable => "Temperature not stable",
            Error::TemperatureOutOfBounds(message) => message,
            Error::UnableToPerformTest(message) => message,
            Error::InsufficientData(message) => message,
        };
        write!(f, "{}", message)
    }
}

impl std::error::Error for Error {}

#[derive(Default)]
pub struct HeuristicAutoTuner {
    sample_time: Duration,
    temperature_samples: Vec<f32>,
    sample_count: usize,
    sample_distance: usize,
    time_to_halfway_point: Option<Duration>,
    elapsed_time_heating: Option<Duration>,
    ambient_temperature: Option<f32>,
    boiler_simulator: BoilerModel,
    differential_data: DifferentialData,
    results: Option<BoilerModelParameters>,
}

impl HeuristicAutoTuner {
    pub fn new(sample_time: Duration) -> Self {
        let mut boiler_simulator = BoilerModel::new(Some(25.0));
        boiler_simulator.max_power = HEATER_MAX_POWER;
        Self {
            sample_time,
            temperature_samples: vec![0.0; 16],
            sample_count: 0,
            sample_distance: 1,
            boiler_simulator,
            ..Default::default()
        }
    }

    pub fn get_probe(&self) -> f32 {
        self.boiler_simulator.get_noisy_probe()
    }

    fn get_interval(&self) -> usize {
        self.sample_distance * (self.sample_count / 2)
    }

    pub fn get_3_samples(&self) -> Option<(f32, f32, f32)> {
        if self.sample_count < 3 {
            return None;
        }

        let first = self.temperature_samples[0];
        let second = self.temperature_samples[(self.sample_count - 1) / 2];
        let third = self.temperature_samples[self.sample_count - 1];

        Some((first, second, third))
    }

    fn settle_down(&mut self, target: f32) {
        let test_interval = if Duration::from_secs(1) > self.sample_time {
            Duration::from_secs(1)
        } else {
            self.sample_time
        };

        let mut current_time = Instant::now();
        let mut next_test_time = current_time + test_interval;

        loop {
            self.boiler_simulator
                .update(HEATER_MAX_POWER / 2.0, test_interval);
            current_time += test_interval;

            if current_time >= next_test_time {
                let current_temp = self.get_probe();
                if current_temp > target + 1.0 {
                    break;
                }
                next_test_time += test_interval;
            }
        }
        loop {
            self.boiler_simulator.update(0.0, test_interval);
            current_time += test_interval;

            if current_time >= next_test_time {
                let current_temp = self.get_probe();
                if current_temp <= target {
                    break;
                }
                next_test_time += test_interval;
            }
        }
    }

    pub fn measure_ambient(
        &mut self,
        duration: Duration,
        max_delta: f32,
        timeout: Option<Duration>,
    ) -> Result<f32, Error> {
        let mut retries = timeout.map(|t| t.as_millis() / duration.as_millis());

        loop {
            let mut samples: [f32; 2] = [0.0, 0.0];

            samples[0] = self.get_probe();
            self.boiler_simulator.update(0.0, duration);
            samples[1] = self.get_probe();

            let delta = (samples[1] - samples[0]).abs();
            if delta < max_delta {
                /* Check we're not still cooling down */
                if samples[0] <= samples[1] {
                    log::trace!("Ambient temperature samples: {:?}", samples);
                    self.ambient_temperature = Some((samples[0] + samples[1]) / 2.0);
                    log::debug!("Ambient temperature: {}", self.ambient_temperature.unwrap());
                    return Ok(self.ambient_temperature.unwrap());
                }
            }

            if let Some(remaining) = retries {
                if remaining == 0 {
                    return Err(Error::TemperatureNotStable);
                } else {
                    retries = Some(remaining - 1);
                }
            }
        }
    }

    fn measure_heatup(&mut self, target: f32) -> Result<(), Error> {
        let test_interval = if Duration::from_secs(1) > self.sample_time {
            Duration::from_secs(1)
        } else {
            self.sample_time
        };

        self.sample_count = 0;
        self.sample_distance = 1;
        self.time_to_halfway_point = None;
        self.elapsed_time_heating = None;
        self.differential_data = DifferentialData::default();
        let mut current_time = Instant::now();
        self.time_to_halfway_point = None;

        let start_time = Instant::now();
        let mut next_test_time = start_time + self.sample_time;

        let current_temperature = self.get_probe();
        for i in 0..3 {
            self.temperature_samples[i] = current_temperature;
        }

        loop {
            self.boiler_simulator
                .update(HEATER_MAX_POWER, test_interval);
            current_time += test_interval;

            if current_time >= next_test_time {
                let current_temperature = self.get_probe();

                if current_temperature < target / 2.0 {
                    self.temperature_samples[0] = self.temperature_samples[1];
                    self.temperature_samples[1] = self.temperature_samples[2];
                    self.temperature_samples[2] = current_temperature;

                    let current_slope = (self.temperature_samples[2] - self.temperature_samples[0])
                        / (2.0 * test_interval.as_secs_f32());

                    if current_slope > self.differential_data.rate {
                        self.differential_data.rate = current_slope;
                        self.differential_data.temperature = self.temperature_samples[1];
                        self.differential_data.time = Some(current_time - test_interval);
                    }
                } else if current_temperature < target {
                    if self.sample_count == 0 {
                        self.time_to_halfway_point = Some(current_time - start_time);
                    }

                    /* Double sample spacing if we are out of samples */
                    if self.sample_count == self.temperature_samples.len() {
                        for i in 0..(self.temperature_samples.len() / 2) {
                            self.temperature_samples[i] = self.temperature_samples[i * 2];
                        }
                        self.sample_distance *= 2;
                        self.sample_count /= 2;
                    }

                    self.temperature_samples[self.sample_count] = current_temperature;
                    self.sample_count += 1;
                } else {
                    self.elapsed_time_heating = Some(current_time - start_time);

                    if self.sample_count == 0 {
                        return Err(Error::UnableToPerformTest(
                            "Need to be well below the target to perform the heatup test"
                                .to_string(),
                        ));
                    } else if self.sample_count % 2 == 0 {
                        self.sample_count -= 1;
                    }
                    log::trace!("Heatup samples: {:?}", self.temperature_samples);
                    log::trace!("Elapsed time heating: {:?}", self.elapsed_time_heating);
                    return Ok(());
                }
                next_test_time += test_interval * self.sample_distance as u32;
            }
        }
    }

    fn measure_ambient_transfer(
        &mut self,
        test_duration: Duration,
        settle_time: Duration,
        target: f32,
    ) -> Result<f32, Error> {
        let test_interval = if Duration::from_secs(1) > self.sample_time {
            Duration::from_secs(1)
        } else {
            self.sample_time
        };

        let boiler_heat_capacity = self
            .results
            .as_ref()
            .ok_or(Error::InsufficientData(
                "Need to estimate values from heatup test first".to_string(),
            ))?
            .thermal_mass;

        let start_time = Instant::now();
        let mut next_test_time = start_time + test_interval;
        let settle_time_end = start_time + settle_time;
        let test_end_time = settle_time_end + test_duration;

        let mut total_energy = 0.0;
        let mut heater_power = TRANSFER_TEST_HEATER_POWER;
        let mut current_time = Instant::now();
        let mut previous_temperature = self.get_probe();

        loop {
            current_time += self.sample_time;

            if current_time >= next_test_time {
                self.boiler_simulator.update(heater_power, test_interval);
                let current_temperature = self.get_probe();

                if current_time < test_end_time && current_time >= settle_time_end {
                    let energy = heater_power * test_interval.as_secs_f32()
                        + (previous_temperature - current_temperature) * boiler_heat_capacity;
                    total_energy += energy;
                } else if current_time >= test_end_time {
                    log::trace!("Total energy: {}", total_energy);
                    log::trace!("Test duration: {}", test_duration.as_secs_f32());
                    return Ok(total_energy / test_duration.as_secs_f32());
                }

                if self.temperature_samples[2] - 15.0 >= current_temperature {
                    return Err(Error::TemperatureOutOfBounds(format!(
                        "Temperature out of bounds: {} lower tham limit of {} ❄",
                        current_temperature,
                        self.temperature_samples[2] - 15.0
                    )));
                } else if current_temperature >= target + 15.0 {
                    return Err(Error::TemperatureOutOfBounds(format!(
                        "Temperature out of bounds: {} higher than limit of{} 🔥",
                        current_temperature,
                        target + 15.0
                    )));
                }

                previous_temperature = current_temperature;
                next_test_time += test_interval;

                // just bitbang for now. In the real implementation, activate MPC with the estimated values
                if current_temperature >= target {
                    heater_power = 0.0;
                } else {
                    heater_power = TRANSFER_TEST_HEATER_POWER;
                }
            }
        }
    }

    fn estimate_values_from_heatup(&mut self) -> Result<f32, Error> {
        let (s0, s1, s2) = self.get_3_samples().ok_or(Error::InsufficientData(
            "Need at least 3 samples to estimate values".to_string(),
        ))?;

        log::debug!("s0: {}, s1: {}, s2: {}", s0, s1, s2);
        log::debug!("Spacing: {}", self.get_interval());

        let asymptotic_temperature = (s1 * s1 - s0 * s2) / (2.0 * s1 - s0 - s2);
        let boiler_responsiveness =
            f32::ln((s0 - asymptotic_temperature) / (s1 - asymptotic_temperature))
                / self.get_interval() as f32;

        let ambient_temperature = self.ambient_temperature.ok_or(Error::InsufficientData(
            "Requires an ambient temperature is measured first".to_string(),
        ))?;

        log::debug!(
            "asymptotic_temperature: {}, boiler_responsiveness: {}",
            asymptotic_temperature,
            boiler_responsiveness
        );

        let ambient_transfer_coefficient =
            self.boiler_simulator.max_power / (asymptotic_temperature - ambient_temperature);

        let boiler_thermal_mass = ambient_transfer_coefficient / boiler_responsiveness;

        let first_temperature_sample_time = self
            .time_to_halfway_point
            .ok_or(Error::InsufficientData(
                "Need to get the time of the first sample".to_string(),
            ))?
            .as_secs_f32();

        let probe_responsiveness = boiler_responsiveness
            / (1.0
                - (ambient_temperature - asymptotic_temperature)
                    * (-boiler_responsiveness * first_temperature_sample_time).exp()
                    / (s0 - asymptotic_temperature));

        self.results = Some(BoilerModelParameters {
            thermal_mass: boiler_thermal_mass,
            ambient_transfer_coefficient,
            probe_responsiveness,
        });

        log::info!("Estimated values:");
        self.print_results();

        let elapsed_time_heating = self
            .elapsed_time_heating
            .ok_or(Error::InsufficientData(
                "Heatup test needs to be completed first".to_string(),
            ))?
            .as_secs_f32();
        log::trace!("Elapsed time heating: {}", elapsed_time_heating);

        let estimated_temperature = asymptotic_temperature
            + (ambient_temperature - asymptotic_temperature)
                * (-boiler_responsiveness * elapsed_time_heating).exp();

        log::trace!("Estimated temperature: {}", estimated_temperature);
        log::trace!(
            "Actual temperature: {}",
            self.boiler_simulator.get_actual_temperature()
        );
        Ok(estimated_temperature)
    }

    fn estimate_values_from_thermal_transfer(
        &mut self,
        power: f32,
        target_temperature: f32,
    ) -> Result<BoilerModelParameters, Error> {
        let ambient_temperature = self.ambient_temperature.ok_or(Error::InsufficientData(
            "Need to measure ambient temperature first".to_string(),
        ))?;

        let ambient_transfer_coefficient = power / (target_temperature - ambient_temperature);

        let asymptotic_temperature =
            ambient_temperature + HEATER_MAX_POWER / ambient_transfer_coefficient;
        let (s0, s1, _) = self.get_3_samples().ok_or(Error::InsufficientData(
            "Need at least 3 samples to estimate values".to_string(),
        ))?;
        let boiler_responsiveness =
            f32::ln((s0 - asymptotic_temperature) / (s1 - asymptotic_temperature))
                / self.get_interval() as f32;

        let boiler_thermal_mass = ambient_transfer_coefficient / boiler_responsiveness;

        let first_temperature_sample_time = self
            .time_to_halfway_point
            .ok_or(Error::InsufficientData(
                "Need to get the time of the first sample".to_string(),
            ))?
            .as_secs_f32();
        let probe_responsiveness = boiler_responsiveness
            / (1.0
                - (ambient_temperature - asymptotic_temperature)
                    * (-boiler_responsiveness * first_temperature_sample_time).exp()
                    / (s0 - asymptotic_temperature));

        Ok(BoilerModelParameters {
            thermal_mass: boiler_thermal_mass,
            ambient_transfer_coefficient,
            probe_responsiveness,
        })
    }

    pub fn print_results(&self) {
        let actual_params = self.boiler_simulator.parameters;
        log::info!("Actual values \n{}", actual_params);

        if let Some(results) = &self.results {
            log::info!("Estimated values:\n{}", results);

            log::info!(
                "Error percent:\n{}",
                [
                    (
                        "Thermal Mass",
                        (actual_params.thermal_mass - results.thermal_mass).abs()
                            / actual_params.thermal_mass
                            * 100.0
                    ),
                    (
                        "Ambient Transfer Coefficient",
                        (actual_params.ambient_transfer_coefficient
                            - results.ambient_transfer_coefficient)
                            .abs()
                            / actual_params.ambient_transfer_coefficient
                            * 100.0
                    ),
                    (
                        "Probe Responsiveness",
                        (actual_params.probe_responsiveness - results.probe_responsiveness).abs()
                            / actual_params.probe_responsiveness
                            * 100.0
                    )
                ]
                .iter()
                .map(|(label, x)| format!("{}: {:.2}%", label, x))
                .collect::<Vec<String>>()
                .join("\n")
            );
            log::info!("");
        }
    }

    pub fn auto_tune() -> Result<(), Error> {
        let mut auto_tuner = HeuristicAutoTuner::new(Duration::from_millis(1000));

        log::debug!("Measurt ambient temperature");
        auto_tuner.measure_ambient(Duration::from_secs(60), 1.0, None)?;

        log::debug!("Measuring heatup");
        auto_tuner.measure_heatup(TARGET_TEMPERATURE)?;

        log::debug!("Estimating values from heatup");
        let estimated_temperature = auto_tuner.estimate_values_from_heatup()?;

        auto_tuner.settle_down(estimated_temperature);

        log::debug!("Measuring ambient transfer");
        let power = auto_tuner.measure_ambient_transfer(
            Duration::from_secs(500),
            Duration::from_secs(0),
            estimated_temperature,
        )?;

        log::debug!("Estimating values from thermal transfer");
        let results =
            auto_tuner.estimate_values_from_thermal_transfer(power, estimated_temperature)?;

        auto_tuner.results = Some(results);

        auto_tuner.print_results();

        Ok(())
    }
}
