use std::time::Duration;

pub const LOAD_SENSOR_SCALING: f32 = 4.761905;
pub const SCALE_POLLING_RATE_MS: Duration = Duration::from_millis(10 * 10);
pub const SCALE_SAMPLES: usize = 5;

pub const ADC_POLLING_RATE_MS: Duration = Duration::from_millis(10);
pub const ADC_SAMPLES: usize = 100;

pub const BOILER_PWM_PERIOD: Duration = Duration::from_millis(1000);
pub const PUMP_PWM_PERIOD: Duration = Duration::from_millis(100);
pub const OUTPUT_POLL_INTERVAL: Duration = Duration::from_millis(100);

pub const PT_100_CALIBRATION_FACTOR: f32 = 2.209;

pub const LED_COUNT: usize = 32;
pub const LED_REFRESH_INTERVAL: Duration = Duration::from_millis(100);

pub const _IDLE_TEMPERATURE: f32 = 60.0;
pub const BOILER_POWER: f32 = 2000.0;
pub const INITIAL_TEMPERATURE: f32 = 25.0;
pub const STAND_IN_AMBIENT: f32 = 25.0;

pub const AUTOTUNE_MAX_POWER: f32 = 1000.0;
pub const AUTOTUNE_STEADY_STATE_POWER: f32 = AUTOTUNE_MAX_POWER * 0.5;
pub const AUTOTUNE_TARGET_TEMPERATURE: f32 = 94.0;
pub const STEADY_STATE_TEST_TIME: Duration = Duration::from_secs(600);
#[cfg(feature = "simulate")]
pub const TIME_DILATION_FACTOR: f32 = 0.01;

pub const MPC_SMOOTHING_FACTOR: f32 = 0.5;

pub struct Shots {}

impl Shots {
    pub const MAX_SHOT_TEMPERATURE: f32 = 105.0;
    pub const MIN_SHOT_TEMPERATURE: f32 = 00.0;
    pub const MAX_SHOT_PRESSURE_BAR: f32 = 12.0;
    pub const MIN_SHOT_PRESSURE_BAR: f32 = 3.0;
}
