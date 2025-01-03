use crate::types::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub mqtt: Mqtt,
    pub load_cell: LoadCell,
    pub adc: Adc,
    pub boiler: Boiler,
    pub mpc: Mpc,
    pub pump: Pump,
    pub level_sensor: LevelSensor,
    pub indicator: Indicator,
}

#[derive(Serialize, Deserialize)]
pub struct Mqtt {}
impl Mqtt {
    pub const REPORT_INTERVAL: Duration = Duration::from_secs(2);
    pub const STATUS_TOPIC: &'static str = "dummy/status";
    pub const EVENT_TOPIC: &'static str = "dummy/event";
    pub const EVENT_LEVEL: crate::schemas::event::LevelFilter =
        crate::schemas::event::LevelFilter::Debug;
}

#[derive(Serialize, Deserialize)]
pub struct LoadCell {
    scaling: f32,
    sampling_rate: Duration,
    window: usize,
}
pub const LOAD_SENSOR_SCALING: f32 = 4.761905;
pub const SCALE_POLLING_RATE_MS: Duration = Duration::from_millis(10 * 10);
pub const SCALE_SAMPLES: usize = 5;

#[derive(Serialize, Deserialize)]
pub struct Adc {
    polling_interval: Duration,
    window: usize,
}
pub const ADC_POLLING_RATE_MS: Duration = Duration::from_millis(10);
pub const ADC_SAMPLES: usize = 100;

#[derive(Serialize, Deserialize)]
pub struct Boiler {
    pwm_period: Duration,
    power: Watts,
    pt100_calibration_factor: f32,
}
pub const BOILER_PWM_PERIOD: Duration = Duration::from_millis(1000);
pub const BOILER_POWER: Watts = 2000.0;
pub const PT_100_CALIBRATION_FACTOR: f32 = 2.209;

#[derive(Serialize, Deserialize)]
pub struct Mpc {
    smoothing_factor: f32,
    auto_tune: AutoTune,
}
pub const MPC_SMOOTHING_FACTOR: f32 = 0.5;

#[derive(Serialize, Deserialize)]
pub struct AutoTune {
    max_power: Watts,
    steady_state_power: Watts,
    target_temperature: Temperature,
    test_time: Duration,
}
pub const AUTOTUNE_MAX_POWER: Watts = 1000.0;
pub const AUTOTUNE_STEADY_STATE_POWER: Watts = AUTOTUNE_MAX_POWER * 0.5;
pub const AUTOTUNE_TARGET_TEMPERATURE: Temperature = 94.0;
pub const STEADY_STATE_TEST_TIME: Duration = Duration::from_secs(600);

#[derive(Serialize, Deserialize)]
pub struct Pump {
    pwm_period: Duration,
    max_pressure: Bar,
    backflush_on_time: Duration,
    backflush_off_time: Duration,
}
pub const PUMP_PWM_PERIOD: Duration = Duration::from_millis(100);
pub const MAX_PUMP_PRESSURE: Bar = 15.0;
pub const BACKFLUSH_ON_TIME: Duration = Duration::from_secs(10);
pub const BACKFLUSH_OFF_TIME: Duration = Duration::from_secs(10);

#[derive(Serialize, Deserialize)]
pub struct LevelSensor {
    low_level_threshold: Millimeters,
}
pub const LOW_LEVEL_THRESHOLD: Millimeters = 100;

#[derive(Serialize, Deserialize)]
pub struct Indicator {
    refresh_interval: Duration,
}
pub const LED_COUNT: usize = 32;
pub const LED_REFRESH_INTERVAL: Duration = Duration::from_millis(100);

#[cfg(feature = "simulate")]
pub const TIME_DILATION_FACTOR: f32 = 0.01;
#[cfg(not(feature = "simulate"))]
pub const TIME_DILATION_FACTOR: f32 = 1.0;

pub struct Shots {}

impl Shots {
    pub const MAX_SHOT_TEMPERATURE: f32 = 105.0;
    pub const MIN_SHOT_TEMPERATURE: f32 = 00.0;
    pub const MAX_SHOT_PRESSURE_BAR: f32 = 12.0;
    pub const MIN_SHOT_PRESSURE_BAR: f32 = 3.0;
}
