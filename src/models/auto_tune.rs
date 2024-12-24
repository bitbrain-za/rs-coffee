use esp_idf_hal::delay::FreeRtos;

use crate::models::boiler::{BoilerModel, BoilerModelParameters};
use std::time::{Duration, Instant};

const HEATER_MAX_POWER: f32 = 1000.0;
const TRANSFER_TEST_HEATER_POWER: f32 = HEATER_MAX_POWER * 0.5;
const TARGET_TEMPERATURE: f32 = 94.0;

#[cfg(feature = "simulate")]
const TIME_DILATION_FACTOR: f32 = 0.01;
#[cfg(not(feature = "simulate"))]
const TIME_DILATION_FACTOR: f32 = 1.0;

const SAMPLE_TIME: Duration = Duration::from_secs(1);

fn convert_to_dilated_time(duration: Duration) -> Duration {
    #[cfg(not(feature = "simulate"))]
    return duration;

    let s = duration.as_secs_f32() * TIME_DILATION_FACTOR;
    Duration::from_secs_f32(s)
}

fn convert_to_normal_time(duration: Duration) -> Duration {
    #[cfg(not(feature = "simulate"))]
    return duration;

    let s = duration.as_secs_f32() / TIME_DILATION_FACTOR;
    Duration::from_secs_f32(s)
}

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
enum Mode {
    #[default]
    Setup,
}

#[derive(Default)]
pub struct HeuristicAutoTuner {
    sample_time: Duration,
    ambient_temperature: Option<f32>,
    boiler_simulator: BoilerModel,
    results: Option<BoilerModelParameters>,
    ambient_measurement: AmbientTest,
}

pub struct AmbientTest {
    initial_sample: f32,
    end_of_settling_time: Instant,
    retries: usize,
}

impl Default for AmbientTest {
    fn default() -> Self {
        Self {
            initial_sample: 0.0,
            end_of_settling_time: Instant::now(),
            retries: 0,
        }
    }
}

pub enum AmbientMeasurementState {
    Busy(Duration),
    Done(f32),
    Err(Error),
}

impl AmbientTest {
    pub fn start(
        &mut self,
        test_duration: Option<Duration>,
        retries: Option<usize>,
        current_temperature: f32,
    ) {
        self.end_of_settling_time = Instant::now()
            + test_duration.unwrap_or(Duration::from_secs_f32(60.0 * TIME_DILATION_FACTOR));

        self.retries = retries.unwrap_or(0);
        self.initial_sample = current_temperature;
    }

    fn sample(&mut self, current_probe: f32) -> AmbientMeasurementState {
        if Instant::now() >= self.end_of_settling_time {
            if (current_probe - self.initial_sample).abs() < 1.0 {
                AmbientMeasurementState::Done((self.initial_sample + current_probe) / 2.0)
            } else if self.retries > 0 {
                self.retries -= 1;
                self.end_of_settling_time = Instant::now() + Duration::from_secs(10);
                AmbientMeasurementState::Busy(Duration::from_secs(10))
            } else {
                AmbientMeasurementState::Err(Error::TemperatureNotStable)
            }
        } else {
            AmbientMeasurementState::Busy(self.end_of_settling_time - Instant::now())
        }
    }
}

#[derive(Default)]
struct HeatupTest {
    target: f32,

    temperature_samples: Vec<f32>,
    sample_count: usize,
    sample_distance: usize,

    sample_time: Duration,
    time_to_halfway_point: Duration,
    differential_data: DifferentialData,
    next_test_time: Option<Instant>,
    test_interval: Duration,
    start_time: Option<Instant>,
}

enum HeatupTestState {
    Busy,
    Done(HeatupTestData),
    Err(Error),
}

struct HeatupTestData {
    temperature_samples: Vec<f32>,
    sample_count: usize,
    sample_distance: usize,
    time_to_halfway_point: Duration,

    // used?
    power: f32,
    elapsed_time_heating: Duration,
}

impl HeatupTestData {
    fn get_interval(&self) -> usize {
        self.sample_distance * (self.sample_count / 2)
    }

    fn get_3_samples(&self) -> Option<(f32, f32, f32)> {
        if self.sample_count < 3 {
            return None;
        }

        let first = self.temperature_samples[0];
        let second = self.temperature_samples[(self.sample_count - 1) / 2];
        let third = self.temperature_samples[self.sample_count - 1];

        Some((first, second, third))
    }

    fn estimate_values_from_heatup(
        &mut self,
        ambient_temperature: f32,
    ) -> Result<(f32, BoilerModelParameters), Error> {
        let (s0, s1, s2) = self.get_3_samples().ok_or(Error::InsufficientData(
            "Need at least 3 samples to estimate values".to_string(),
        ))?;

        log::debug!("s0: {}, s1: {}, s2: {}", s0, s1, s2);
        log::debug!("Spacing: {}", self.get_interval());

        let asymptotic_temperature = (s1 * s1 - s0 * s2) / (2.0 * s1 - s0 - s2);
        let boiler_responsiveness =
            f32::ln((s0 - asymptotic_temperature) / (s1 - asymptotic_temperature))
                / self.get_interval() as f32;

        log::debug!(
            "asymptotic_temperature: {}, boiler_responsiveness: {}",
            asymptotic_temperature,
            boiler_responsiveness
        );

        let ambient_transfer_coefficient =
            self.power / (asymptotic_temperature - ambient_temperature);

        let boiler_thermal_mass = ambient_transfer_coefficient / boiler_responsiveness;

        let first_temperature_sample_time = self.time_to_halfway_point.as_secs_f32();

        let probe_responsiveness = boiler_responsiveness
            / (1.0
                - (ambient_temperature - asymptotic_temperature)
                    * (-boiler_responsiveness * first_temperature_sample_time).exp()
                    / (s0 - asymptotic_temperature));

        let mpc = BoilerModelParameters {
            thermal_mass: boiler_thermal_mass,
            ambient_transfer_coefficient,
            probe_responsiveness,
        };

        let elapsed_time_heating = self.elapsed_time_heating.as_secs_f32() / TIME_DILATION_FACTOR;

        let estimated_temperature = asymptotic_temperature
            + (ambient_temperature - asymptotic_temperature)
                * (-boiler_responsiveness * elapsed_time_heating).exp();

        log::debug!("Estimated temperature: {}", estimated_temperature);
        log::debug!("Estimated values: {:?}", mpc);
        log::debug!("Time to 50%: {:.2}", first_temperature_sample_time);
        log::debug!("Elapsed time heating: {}", elapsed_time_heating);

        Ok((estimated_temperature, mpc))
    }
}

impl HeatupTest {
    fn start(&mut self, current_temperature: f32, target: f32) {
        self.test_interval = if Duration::from_secs(1) > self.sample_time {
            Duration::from_secs(1)
        } else {
            self.sample_time
        };

        self.test_interval = convert_to_dilated_time(self.test_interval);
        self.sample_count = 0;
        self.sample_distance = 1;
        self.differential_data = DifferentialData::default();
        self.temperature_samples = vec![0.0; 16];
        let start_time = Instant::now();
        self.start_time = Some(start_time);
        self.next_test_time = Some(start_time + self.test_interval);
        self.target = target;

        for i in 0..3 {
            self.temperature_samples[i] = current_temperature;
        }
    }

    fn measure(&mut self, current_temperature: f32) -> HeatupTestState {
        let current_time = Instant::now();

        if self.next_test_time.is_none() || self.start_time.is_none() {
            return HeatupTestState::Err(Error::UnableToPerformTest(
                "Need to start the test first".to_string(),
            ));
        }
        let next_time = self.next_test_time.unwrap();
        let start_time = self.start_time.unwrap();

        if current_time >= next_time {
            log::trace!(
                "Sample @ {:?}s = {} degees",
                current_time,
                current_temperature
            );

            if current_temperature < self.target / 2.0 {
                self.temperature_samples[0] = self.temperature_samples[1];
                self.temperature_samples[1] = self.temperature_samples[2];
                self.temperature_samples[2] = current_temperature;

                let current_slope = (self.temperature_samples[2] - self.temperature_samples[0])
                    / (2.0 * self.test_interval.as_secs_f32());

                if current_slope > self.differential_data.rate {
                    self.differential_data.rate = current_slope;
                    self.differential_data.temperature = self.temperature_samples[1];
                    self.differential_data.time = Some(current_time - self.test_interval);
                }
            } else if current_temperature < self.target {
                if self.sample_count == 0 {
                    self.time_to_halfway_point = current_time - start_time;
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
                let elapsed_time_heating = current_time - start_time;

                if self.sample_count == 0 {
                    return HeatupTestState::Err(Error::UnableToPerformTest(
                        "Need to be well below the target to perform the heatup test".to_string(),
                    ));
                } else if self.sample_count % 2 == 0 {
                    self.sample_count -= 1;
                }
                log::trace!("Heatup samples: {:?}", self.temperature_samples);
                log::trace!("Elapsed time heating: {:?}", elapsed_time_heating);
                return HeatupTestState::Done(HeatupTestData {
                    temperature_samples: self.temperature_samples.clone(),
                    sample_count: self.sample_count,
                    sample_distance: self.sample_distance,
                    power: HEATER_MAX_POWER,
                    time_to_halfway_point: Duration::from_secs_f32(
                        self.time_to_halfway_point.as_secs_f32() / TIME_DILATION_FACTOR,
                    ),
                    elapsed_time_heating,
                });
            }
            self.next_test_time =
                Some(next_time + self.test_interval * self.sample_distance as u32);
        }

        HeatupTestState::Busy
    }
}

struct AmbientTransferTest {
    heatup_test_data: HeatupTestData,

    mpc: BoilerModelParameters,
    target: f32,

    total_energy: f32,
    previous_temperature: f32,

    last_test_instant: Instant,
    test_duration: Duration,
    settle_time: Duration,

    start_time: Option<Instant>,
    accumulation_time_s: f32,
}

enum AmbientTransferTestState {
    Busy,
    Done(f32),
    Err(Error),
}

impl AmbientTransferTest {
    fn new(data: HeatupTestData, ambient_temperature: f32) -> Result<Self, Error> {
        let mut data = data;
        let (target, mpc) = data.estimate_values_from_heatup(ambient_temperature)?;
        Ok(Self {
            heatup_test_data: data,
            mpc,
            target,
            accumulation_time_s: 0.0,
            previous_temperature: 0.0,

            last_test_instant: Instant::now(),
            test_duration: Duration::from_secs(500),
            settle_time: Duration::from_secs(0),

            start_time: None,
            total_energy: 0.0,
        })
    }
    fn get_dilated_test_duration(&self) -> Duration {
        convert_to_dilated_time(self.test_duration)
    }
    fn get_dilated_settle_time(&self) -> Duration {
        convert_to_dilated_time(self.settle_time)
    }
    fn elapsed_as_secs_f32_with_dilation(instant: Instant) -> f32 {
        instant.elapsed().as_secs_f32() / TIME_DILATION_FACTOR
    }
    fn start(&mut self, test_duration: Duration, settle_time: Duration) {
        self.test_duration = test_duration + settle_time;
        self.settle_time = settle_time;
    }

    fn measure(&mut self, heater_power: f32, current_temperature: f32) -> AmbientTransferTestState {
        if self.start_time.is_none() {
            self.previous_temperature = current_temperature;
            self.start_time = Some(Instant::now());
            self.last_test_instant = Instant::now();
            self.accumulation_time_s = 0.0;
            self.total_energy = 0.0;
            return AmbientTransferTestState::Busy;
        }

        let start_time = self.start_time.unwrap();
        let elapsed = start_time.elapsed();

        let test_duration = self.get_dilated_test_duration();
        let settle_time = self.get_dilated_settle_time();

        if elapsed < test_duration && elapsed >= settle_time {
            let delta_time = Self::elapsed_as_secs_f32_with_dilation(self.last_test_instant);
            let energy = heater_power * delta_time
                + (self.previous_temperature - current_temperature) * self.mpc.thermal_mass;
            self.total_energy += energy;
            self.accumulation_time_s += delta_time;
        } else if elapsed >= (settle_time + test_duration) {
            // (above) yes, we don't need to add these, but keep this for readability
            log::debug!("Total energy: {}", self.total_energy);
            self.test_duration = Duration::from_secs_f32(self.accumulation_time_s);
            log::debug!("Test duration: {}", self.test_duration.as_secs_f32());
            return AmbientTransferTestState::Done(self.power());
        }
        self.previous_temperature = current_temperature;
        self.last_test_instant = Instant::now();

        if self.heatup_test_data.temperature_samples[2] - 15.0 >= current_temperature {
            return AmbientTransferTestState::Err(Error::TemperatureOutOfBounds(format!(
                "Temperature out of bounds: {} lower tham limit of {} ❄",
                current_temperature,
                self.heatup_test_data.temperature_samples[2] - 15.0
            )));
        } else if current_temperature >= self.target + 15.0 {
            return AmbientTransferTestState::Err(Error::TemperatureOutOfBounds(format!(
                "Temperature out of bounds: {} higher than limit of{} 🔥",
                current_temperature,
                self.target + 15.0
            )));
        }

        AmbientTransferTestState::Busy
    }

    fn power(&self) -> f32 {
        self.total_energy / self.test_duration.as_secs_f32()
    }

    fn estimate_values_from_thermal_transfer(
        &mut self,
        ambient_temperature: f32,
    ) -> Result<BoilerModelParameters, Error> {
        log::debug!("Target: {}, Ambient: {}", self.target, ambient_temperature);
        let ambient_transfer_coefficient = self.power() / (self.target - ambient_temperature);

        let asymptotic_temperature =
            ambient_temperature + HEATER_MAX_POWER / ambient_transfer_coefficient;
        log::debug!("Asymptotic temperature: {}", asymptotic_temperature);

        let (s0, s1, _) = self
            .heatup_test_data
            .get_3_samples()
            .ok_or(Error::InsufficientData(
                "Need at least 3 samples to estimate values".to_string(),
            ))?;
        let boiler_responsiveness =
            f32::ln((s0 - asymptotic_temperature) / (s1 - asymptotic_temperature))
                / self.heatup_test_data.get_interval() as f32;

        log::debug!("Boiler responsiveness: {:+e}", boiler_responsiveness);
        log::debug!("Interval: {}", self.heatup_test_data.get_interval());
        let boiler_thermal_mass = ambient_transfer_coefficient / boiler_responsiveness;

        let first_temperature_sample_time =
            self.heatup_test_data.time_to_halfway_point.as_secs_f32();
        log::debug!("first sample time: {}", first_temperature_sample_time);
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
}

impl HeuristicAutoTuner {
    pub fn new(sample_time: Duration) -> Self {
        let mut boiler_simulator = BoilerModel::new(Some(25.0));
        boiler_simulator.max_power = HEATER_MAX_POWER;
        Self {
            sample_time,
            boiler_simulator,
            ..Default::default()
        }
    }

    pub fn get_probe(&self) -> f32 {
        // self.boiler_simulator.get_noisy_probe()
        self.boiler_simulator.probe_temperature
    }

    fn settle_down(&mut self, target: f32) {
        let test_interval = Duration::from_secs(1);

        let mut current_time = Instant::now();
        let mut next_test_time = current_time + test_interval;

        loop {
            self.boiler_simulator
                .update(HEATER_MAX_POWER / 2.0, test_interval);
            current_time += test_interval;

            if current_time >= next_test_time {
                let current_temp = self.get_probe();
                if current_temp > target + 0.5 {
                    log::debug!("Settling up @ {:?}s", current_time);
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
                    log::debug!("Settling down @ {:?}s", current_time);
                    break;
                } else {
                    log::trace!("Settling down from {} to {}", current_temp, target);
                }
                next_test_time += test_interval;
            }
        }
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

    pub fn auto_tune(&mut self) -> Result<(), Error> {
        log::info!("Measuring ambient temperature");
        self.ambient_measurement.start(None, None, self.get_probe());
        let dt = self.sample_time;
        loop {
            let power = 0.0;
            FreeRtos::delay_ms(
                (1000.0 * (self.sample_time.as_secs_f32() * TIME_DILATION_FACTOR)) as u32,
            );
            self.boiler_simulator.update(power, dt);

            match self.ambient_measurement.sample(self.get_probe()) {
                AmbientMeasurementState::Done(ambient_temperature) => {
                    self.ambient_temperature = Some(ambient_temperature);
                    #[cfg(feature = "simulate")]
                    {
                        self.boiler_simulator.ambient_temperature = ambient_temperature;
                    }
                    log::debug!(
                        "Ambient Temperature = {}",
                        self.boiler_simulator.ambient_temperature
                    );
                    break;
                }
                AmbientMeasurementState::Err(e) => return Err(e),
                _ => {}
            }
        }

        log::info!("Measuring heatup");

        let mut heatup_test = HeatupTest {
            sample_time: self.sample_time,
            ..Default::default()
        };
        heatup_test.start(self.get_probe(), TARGET_TEMPERATURE);
        let mut heatup_results: HeatupTestData;
        loop {
            let power = 1000.0;
            FreeRtos::delay_ms(
                (1000.0 * (self.sample_time.as_secs_f32() * TIME_DILATION_FACTOR)) as u32,
            );
            self.boiler_simulator.update(power, dt);

            match heatup_test.measure(self.get_probe()) {
                HeatupTestState::Done(results) => {
                    heatup_results = results;
                    break;
                }
                HeatupTestState::Err(e) => return Err(e),
                _ => {}
            }
        }

        log::info!("Estimating values from heatup");
        let (estimated_temperature, _mpc) =
            heatup_results.estimate_values_from_heatup(self.ambient_temperature.unwrap())?;

        let mut ambient_transfer_test =
            AmbientTransferTest::new(heatup_results, self.ambient_temperature.unwrap())?;

        // [ ] when we have MPC control here, set the first tpass values.
        log::info!("Settling down");
        self.settle_down(ambient_transfer_test.target);

        log::info!("Measuring ambient transfer");
        ambient_transfer_test.start(Duration::from_secs(500), Duration::from_secs(30));

        let mut power = 0.0;
        loop {
            self.boiler_simulator.update(power, dt);
            FreeRtos::delay_ms(
                (1000.0 * (self.sample_time.as_secs_f32() * TIME_DILATION_FACTOR)) as u32,
            );

            let current_temperature = self.get_probe();
            match ambient_transfer_test.measure(power, current_temperature) {
                AmbientTransferTestState::Done(test_power) => {
                    log::debug!("Power: {}", test_power);
                    break;
                }
                AmbientTransferTestState::Err(e) => return Err(e),
                _ => {}
            }

            // just bitbang for now. In the real implementation, activate MPC with the estimated values
            power = if current_temperature >= estimated_temperature {
                0.0
            } else {
                TRANSFER_TEST_HEATER_POWER
            };
        }

        log::info!("Estimating values from thermal transfer");
        let results = ambient_transfer_test
            .estimate_values_from_thermal_transfer(self.ambient_temperature.unwrap())?;

        self.results = Some(results);

        self.print_results();

        Ok(())
    }
}
