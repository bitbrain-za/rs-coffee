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

fn convert_to_dilated_time(duration: Duration) -> Duration {
    #[cfg(not(feature = "simulate"))]
    return duration;

    let s = duration.as_secs_f32() * TIME_DILATION_FACTOR;
    Duration::from_secs_f32(s)
}

fn convert_to_dilated_time_secs_f32(duration: Duration) -> f32 {
    #[cfg(not(feature = "simulate"))]
    return duration.as_secs_f32();

    duration.as_secs_f32() * TIME_DILATION_FACTOR
}

fn convert_to_normal_time_secs_f32(duration: Duration) -> f32 {
    #[cfg(not(feature = "simulate"))]
    return duration.as_secs_f32();

    duration.as_secs_f32() / TIME_DILATION_FACTOR
}

fn elapsed_as_secs_f32_with_dilation(instant: Instant) -> f32 {
    instant.elapsed().as_secs_f32() / TIME_DILATION_FACTOR
}

#[derive(Default)]
pub enum HeuristicAutoTunerState {
    #[default]
    Init,
    MeasureAmbient,
    MeasureHeatingUp(HeatupTest),
    MeasureSteadyState(AmbientTransferTest),
    Done,
}

impl PartialEq for HeuristicAutoTunerState {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (HeuristicAutoTunerState::Init, HeuristicAutoTunerState::Init) => true,
            (HeuristicAutoTunerState::MeasureAmbient, HeuristicAutoTunerState::MeasureAmbient) => {
                true
            }
            (
                HeuristicAutoTunerState::MeasureHeatingUp(_),
                HeuristicAutoTunerState::MeasureHeatingUp(_),
            ) => true,
            (
                HeuristicAutoTunerState::MeasureSteadyState(_),
                HeuristicAutoTunerState::MeasureSteadyState(_),
            ) => true,
            (HeuristicAutoTunerState::Done, HeuristicAutoTunerState::Done) => true,
            _ => false,
        }
    }
}

#[derive(Default, PartialEq, Copy, Clone)]
enum SettlingState {
    #[default]
    Init,
    Cooling,
    Heating,
    Done,
}

#[derive(Debug, Default)]
struct DifferentialData {
    rate: f32,
    temperature: f32,
    time: Option<Instant>,
}

#[derive(Debug, Clone)]
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
    state: HeuristicAutoTunerState,
    sample_time: Duration,
    ambient_temperature: Option<f32>,
    boiler_simulator: BoilerModel,
    results: Option<BoilerModelParameters>,
    ambient_measurement: AmbientTest,
    settling_state: SettlingState,
    current_power: f32,
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
    Busy,
    Done(f32),
    Err(Error),
}

impl AmbientTest {
    pub fn start(
        &mut self,
        test_duration: Duration,
        retries: Option<usize>,
        current_temperature: f32,
    ) {
        let test_duration = convert_to_dilated_time(test_duration);
        self.end_of_settling_time = Instant::now() + test_duration;

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
                AmbientMeasurementState::Busy
            } else {
                AmbientMeasurementState::Err(Error::TemperatureNotStable)
            }
        } else {
            AmbientMeasurementState::Busy
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

        let elapsed_time_heating = convert_to_normal_time_secs_f32(self.elapsed_time_heating);

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
                        convert_to_normal_time_secs_f32(self.time_to_halfway_point),
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

#[derive(Default, Copy, Clone)]
enum SettleMode {
    #[default]
    None,
    Time(Duration),
    Value(f32),
}

struct AmbientTransferTest {
    state: AmbientTransferTestState,
    heatup_test_data: HeatupTestData,

    mpc: BoilerModelParameters,
    target: f32,

    total_energy: f32,
    previous_temperature: f32,

    last_test_instant: Instant,
    test_duration: Duration,
    settle_mode: SettleMode,

    start_time: Option<Instant>,
    accumulation_time_s: f32,
}

#[derive(Clone)]
enum AmbientTransferTestState {
    Init,
    Settling(SettlingState),
    Busy,
    Done(f32),
    Err(Error),
}

impl PartialEq for AmbientTransferTestState {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (AmbientTransferTestState::Init, AmbientTransferTestState::Init) => true,
            (AmbientTransferTestState::Settling(_), AmbientTransferTestState::Settling(_)) => true,
            (AmbientTransferTestState::Busy, AmbientTransferTestState::Busy) => true,
            (AmbientTransferTestState::Done(_), AmbientTransferTestState::Done(_)) => true,
            (AmbientTransferTestState::Err(_), AmbientTransferTestState::Err(_)) => true,
            _ => false,
        }
    }
}

impl AmbientTransferTest {
    fn new(data: HeatupTestData, ambient_temperature: f32) -> Result<Self, Error> {
        let mut data = data;
        let (target, mpc) = data.estimate_values_from_heatup(ambient_temperature)?;
        Ok(Self {
            state: AmbientTransferTestState::Init,
            heatup_test_data: data,
            mpc,
            target,
            accumulation_time_s: 0.0,
            previous_temperature: 0.0,

            last_test_instant: Instant::now(),
            test_duration: Duration::from_secs(500),
            settle_mode: SettleMode::None,

            start_time: None,
            total_energy: 0.0,
        })
    }
    fn get_dilated_test_duration(&self) -> Duration {
        convert_to_dilated_time(self.test_duration)
    }
    fn get_dilated_settle_time(&self) -> Duration {
        if let SettleMode::Time(duration) = self.settle_mode {
            convert_to_dilated_time(duration)
        } else {
            Duration::from_secs(0)
        }
    }

    fn start(&mut self, test_duration: Duration, settle_mode: SettleMode) {
        self.test_duration = test_duration;
        self.state = AmbientTransferTestState::Init;
        match settle_mode {
            SettleMode::Time(settle_time) => {
                self.test_duration = test_duration + settle_time;
            }
            SettleMode::Value(_) => {
                self.state = AmbientTransferTestState::Settling(SettlingState::Init);
            }
            SettleMode::None => {}
        };
        self.settle_mode = settle_mode;
    }

    fn settle_down(&mut self, current_temperature: f32) {
        let test_state = self.state.clone();
        let next = match (test_state, self.settle_mode) {
            (AmbientTransferTestState::Settling(settling_state), SettleMode::Value(target)) => {
                let next_settling_state = match settling_state {
                    SettlingState::Init => {
                        if current_temperature > target {
                            SettlingState::Cooling
                        } else {
                            SettlingState::Heating
                        }
                    }
                    SettlingState::Cooling => {
                        if current_temperature <= target {
                            SettlingState::Done
                        } else {
                            SettlingState::Cooling
                        }
                    }
                    SettlingState::Heating => {
                        if current_temperature > target + 0.5 {
                            SettlingState::Cooling
                        } else {
                            SettlingState::Heating
                        }
                    }
                    SettlingState::Done => SettlingState::Done,
                };
                if next_settling_state == SettlingState::Done {
                    AmbientTransferTestState::Init
                } else {
                    AmbientTransferTestState::Settling(next_settling_state)
                }
            }
            _ => {
                (log::error!("Really shouldn't be able to get here"));
                AmbientTransferTestState::Err(Error::UnableToPerformTest(
                    "Sic hunt draconis".to_string(),
                ))
            }
        };
        self.state = next;
    }

    fn measure(&mut self, heater_power: f32, current_temperature: f32) -> AmbientTransferTestState {
        if self.state == AmbientTransferTestState::Init {
            self.previous_temperature = current_temperature;
            self.start_time = Some(Instant::now());
            self.last_test_instant = Instant::now();
            self.accumulation_time_s = 0.0;
            self.total_energy = 0.0;
            return AmbientTransferTestState::Busy;
        }

        if let AmbientTransferTestState::Settling(state) = self.state {
            if state != SettlingState::Done {
                self.settle_down(current_temperature);
                return AmbientTransferTestState::Busy;
            } else {
                self.state = AmbientTransferTestState::Busy;
            }
        }

        if let AmbientTransferTestState::Err(_) = self.state {
            return self.state.clone();
        }

        let start_time = self.start_time.unwrap();
        let elapsed = start_time.elapsed();

        let test_duration = self.get_dilated_test_duration();
        let settle_time = self.get_dilated_settle_time();

        if elapsed < test_duration && elapsed >= settle_time {
            let delta_time = elapsed_as_secs_f32_with_dilation(self.last_test_instant);
            let energy = heater_power * delta_time
                + (self.previous_temperature - current_temperature) * self.mpc.thermal_mass;
            self.total_energy += energy;
            self.accumulation_time_s += delta_time;
        } else if elapsed >= (settle_time + test_duration) {
            // (above) yes, we don't need to add these, but keep this for readability
            log::debug!("Total energy: {}", self.total_energy);
            self.test_duration = Duration::from_secs_f32(self.accumulation_time_s);
            log::debug!("Test duration: {}", self.test_duration.as_secs_f32());
            self.state = AmbientTransferTestState::Done(self.power());
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

    fn get_probe(&self) -> f32 {
        // self.boiler_simulator.get_noisy_probe()
        self.boiler_simulator.probe_temperature
    }

    fn settle_down(&mut self, target: f32, current_temperature: f32) {
        let next = match &self.settling_state {
            SettlingState::Init => {
                if current_temperature > target {
                    SettlingState::Cooling
                } else {
                    SettlingState::Heating
                }
            }
            SettlingState::Cooling => {
                if current_temperature <= target {
                    SettlingState::Done
                } else {
                    SettlingState::Cooling
                }
            }
            SettlingState::Heating => {
                if current_temperature > target + 0.5 {
                    SettlingState::Cooling
                } else {
                    SettlingState::Heating
                }
            }
            SettlingState::Done => SettlingState::Done,
        };

        self.settling_state = next;
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

    pub fn run(&mut self) -> Result<Option<BoilerModelParameters>, Error> {
        let dt = self.sample_time;

        let current_temperature = self.get_probe();
        let next_state = match self.state {
            HeuristicAutoTunerState::Init => {
                self.results = None;
                log::info!("Measuring ambient temperature");
                self.ambient_measurement
                    .start(Duration::from_secs(60), None, self.get_probe());

                self.current_power = 0.0;
                Some(HeuristicAutoTunerState::MeasureAmbient)
            }
            HeuristicAutoTunerState::MeasureAmbient => {
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

                        log::debug!("Measuring Heatup");
                        let mut heatup_test = HeatupTest {
                            sample_time: self.sample_time,
                            ..Default::default()
                        };
                        heatup_test.start(current_temperature, TARGET_TEMPERATURE);
                        self.current_power = HEATER_MAX_POWER;
                        Some(HeuristicAutoTunerState::MeasureHeatingUp(heatup_test))
                    }
                    AmbientMeasurementState::Err(e) => return Err(e),
                    _ => None,
                }
            }
            HeuristicAutoTunerState::MeasureHeatingUp(ref mut test) => {
                match test.measure(current_temperature) {
                    HeatupTestState::Done(mut heatup_results) => {
                        let (estimated_temperature, _mpc) = heatup_results
                            .estimate_values_from_heatup(self.ambient_temperature.unwrap())?;
                        let mut ambient_transfer_test = AmbientTransferTest::new(
                            heatup_results,
                            self.ambient_temperature.unwrap(),
                        )?;
                        self.current_power = 0.0;
                        ambient_transfer_test.start(
                            Duration::from_secs(500),
                            SettleMode::Value(estimated_temperature),
                        );

                        log::debug!("Running Steady State test");
                        Some(HeuristicAutoTunerState::MeasureSteadyState(
                            ambient_transfer_test,
                        ))
                    }
                    HeatupTestState::Err(e) => return Err(e),
                    _ => None,
                }
            }

            HeuristicAutoTunerState::MeasureSteadyState(ref mut test) => {
                match test.measure(self.current_power, current_temperature) {
                    AmbientTransferTestState::Done(test_power) => {
                        log::debug!("Power: {}", test_power);

                        log::info!("Estimating values from thermal transfer");
                        let results = test.estimate_values_from_thermal_transfer(
                            self.ambient_temperature.unwrap(),
                        )?;

                        self.results = Some(results);
                        self.print_results();

                        Some(HeuristicAutoTunerState::Done)
                    }
                    AmbientTransferTestState::Err(e) => return Err(e),
                    _ => {
                        // [ ] just bitbang for now. In the real implementation, activate MPC with the estimated values
                        self.current_power = if current_temperature >= test.target {
                            0.0
                        } else {
                            TRANSFER_TEST_HEATER_POWER
                        };
                        None
                    }
                }
            }
            HeuristicAutoTunerState::Done => None,
        };
        FreeRtos::delay_ms((1000.0 * convert_to_dilated_time_secs_f32(self.sample_time)) as u32);
        log::trace!(
            "Updating boiler simulator with power: {}, for {:.2}s",
            self.current_power,
            dt.as_secs_f32()
        );
        self.boiler_simulator.update(self.current_power, dt);

        if let Some(state) = next_state {
            self.state = state;
        }
        if self.state == HeuristicAutoTunerState::Done {
            log::info!("Autotune Completed!");
            self.print_results();
        }
        Ok(self.results)
    }

    pub fn simulate(&mut self) -> Result<(), Error> {
        loop {
            if let Some(rees) = self.run()? {
                log::info!("Simulation completed");
                log::info!("Results: {:?}", rees);
                break;
            }
        }
        Ok(())
    }

    pub fn auto_tune(&mut self) -> Result<(), Error> {
        log::info!("Measuring ambient temperature");
        self.ambient_measurement
            .start(Duration::from_secs(60), None, self.get_probe());
        let dt = self.sample_time;
        loop {
            let power = 0.0;
            FreeRtos::delay_ms(
                (1000.0 * convert_to_dilated_time_secs_f32(self.sample_time)) as u32,
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
                (1000.0 * convert_to_dilated_time_secs_f32(self.sample_time)) as u32,
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
        self.settling_state = SettlingState::Init;
        loop {
            let mut power = 0.0;

            self.settle_down(ambient_transfer_test.target, self.get_probe());
            match self.settling_state {
                SettlingState::Cooling => {
                    power = 0.0;
                }
                SettlingState::Heating => {
                    power = 100.0;
                }
                SettlingState::Init => {
                    log::error!("Really shouldn't be able to get here");
                }
                SettlingState::Done => {
                    break;
                }
            }
            self.boiler_simulator.update(power, dt);
            FreeRtos::delay_ms(
                (1000.0 * convert_to_dilated_time_secs_f32(self.sample_time)) as u32,
            );
        }

        log::info!("Measuring ambient transfer");
        ambient_transfer_test.start(
            Duration::from_secs(500),
            SettleMode::Value(estimated_temperature),
        );

        let mut power = 0.0;
        loop {
            self.boiler_simulator.update(power, dt);
            FreeRtos::delay_ms(
                (1000.0 * convert_to_dilated_time_secs_f32(self.sample_time)) as u32,
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

            // [ ] just bitbang for now. In the real implementation, activate MPC with the estimated values
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
