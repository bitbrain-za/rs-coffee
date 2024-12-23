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

pub const IDLE_TEMPERATURE: f32 = 60.0;
pub const BOILER_POWER: f32 = 2000.0;
pub const INITIAL_TEMPERATURE: f32 = 25.0;
