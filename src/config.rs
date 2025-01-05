use crate::kv_store::*;
use crate::types::*;
use dotenv_codegen::dotenv;
use esp_idf_svc::nvs::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Config {
    pub mqtt: Mqtt,
    pub load_cell: LoadCell,
    pub adc: Adc,
    pub boiler: Boiler,
    pub pump: Pump,
    pub level_sensor: LevelSensor,
    pub indicator: Indicator,

    #[serde(skip)]
    pub nvs: Option<EspDefaultNvsPartition>,
}

impl Config {
    pub fn load_or_default(nvs: &Option<EspDefaultNvsPartition>) -> Self {
        match Self::try_load(nvs) {
            Ok(config) => config,
            Err(e) => {
                log::error!("Failed to load config: {:?}, creating a default", e);
                let cfg = Self {
                    nvs: nvs.clone(),
                    ..Default::default()
                };

                if let Err(e) = cfg.save() {
                    log::error!("Failed to save default config: {:?}", e);
                }
                cfg
            }
        }
    }

    pub fn try_load(nvs: &Option<EspDefaultNvsPartition>) -> Result<Self, Error> {
        let fs = KeyValueStore::new(nvs.clone())?;
        let config = FileType::Config.load(&fs)?;

        match config {
            File::Config(mut config) => {
                config.nvs = nvs.clone();
                Ok(config)
            }
            #[allow(unreachable_patterns)]
            _ => Err(Error::NotFound("Config".to_string())),
        }
    }

    pub fn save(&self) -> Result<(), Error> {
        let mut fs = KeyValueStore::new(self.nvs.clone())?;
        File::Config(self.clone().to_owned()).save(&mut fs)
    }

    pub fn update(&mut self, new: Config) -> Result<(), Error> {
        let mut new = new;
        new.nvs = self.nvs.clone();
        *self = new;
        self.save()
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Mqtt {
    pub report_interval: Duration,
    pub status_topic: String,
    pub event_topic: String,
    pub event_level: crate::schemas::event::LevelFilter,
    pub broker: String,
    pub client_id: String,
    pub username: String,
    pub password: String,
    pub port: u16,
}

impl Default for Mqtt {
    fn default() -> Self {
        const DEFAULT_REPORT_INTERVAL: Duration = Duration::from_secs(2);
        const DEFAULT_EVENT_LEVEL: crate::schemas::event::LevelFilter =
            crate::schemas::event::LevelFilter::Info;
        let client_id = dotenv!("MQTT_CLIENT_ID").to_string();
        Mqtt {
            report_interval: DEFAULT_REPORT_INTERVAL,
            status_topic: format!("{}/{}", client_id, "status"),
            event_topic: format!("{}/{}", client_id, "event"),
            event_level: DEFAULT_EVENT_LEVEL,
            broker: dotenv!("MQTT_SERVER").try_into().expect("Invalid MQTT URL"),
            client_id,
            username: dotenv!("MQTT_USER").to_string(),
            password: dotenv!("MQTT_PASSWORD").to_string(),
            port: dotenv!("MQTT_PORT").parse().expect("Invalid MQTT Port"),
        }
    }
}

impl Mqtt {
    pub fn url(&self) -> String {
        format!("mqtt://{}:{}", self.broker, self.port)
    }
}

#[derive(Serialize, Deserialize, Copy, Clone)]
pub struct LoadCell {
    pub scaling: f32,
    pub sampling_rate: Duration,
    pub window: usize,
}

impl Default for LoadCell {
    fn default() -> Self {
        const LOAD_SENSOR_SCALING: f32 = 4.761905;
        const SCALE_POLLING_RATE_MS: Duration = Duration::from_millis(10 * 10);
        const SCALE_SAMPLES: usize = 5;

        LoadCell {
            scaling: LOAD_SENSOR_SCALING,
            sampling_rate: SCALE_POLLING_RATE_MS,
            window: SCALE_SAMPLES,
        }
    }
}

#[derive(Serialize, Deserialize, Copy, Clone)]
pub struct Adc {
    pub polling_interval: Duration,
    pub window: usize,
}

impl Default for Adc {
    fn default() -> Self {
        const ADC_POLLING_RATE_MS: Duration = Duration::from_millis(10);
        const ADC_SAMPLES: usize = 100;

        Adc {
            polling_interval: ADC_POLLING_RATE_MS,
            window: ADC_SAMPLES,
        }
    }
}

#[derive(Serialize, Deserialize, Copy, Clone)]
pub struct Boiler {
    pub pwm_period: Duration,
    pub power: Watts,
    pub pt100_calibration_factor: f32,
    pub mpc: Mpc,
}

impl Default for Boiler {
    fn default() -> Self {
        const BOILER_PWM_PERIOD: Duration = Duration::from_millis(1000);
        const BOILER_POWER: Watts = 2000.0;
        const PT_100_CALIBRATION_FACTOR: f32 = 2.209;

        Boiler {
            pwm_period: BOILER_PWM_PERIOD,
            power: BOILER_POWER,
            pt100_calibration_factor: PT_100_CALIBRATION_FACTOR,
            mpc: Mpc::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Copy, Clone)]
pub struct Mpc {
    pub smoothing_factor: f32,
    pub auto_tune: AutoTune,
    pub parameters: crate::models::boiler::BoilerModelParameters,
}
impl Default for Mpc {
    fn default() -> Self {
        pub const MPC_SMOOTHING_FACTOR: f32 = 0.5;
        Mpc {
            smoothing_factor: MPC_SMOOTHING_FACTOR,
            auto_tune: AutoTune::default(),
            parameters: crate::models::boiler::BoilerModelParameters::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Copy, Clone)]
pub struct AutoTune {
    pub max_power: Watts,
    pub steady_state_power: Watts,
    pub target_temperature: Temperature,
    pub steady_state_test_time: Duration,
}
impl Default for AutoTune {
    fn default() -> Self {
        const AUTOTUNE_MAX_POWER: Watts = 1000.0;
        const AUTOTUNE_STEADY_STATE_POWER: Watts = AUTOTUNE_MAX_POWER * 0.5;
        const AUTOTUNE_TARGET_TEMPERATURE: Temperature = 94.0;
        const STEADY_STATE_TEST_TIME: Duration = Duration::from_secs(600);
        AutoTune {
            max_power: AUTOTUNE_MAX_POWER,
            steady_state_power: AUTOTUNE_STEADY_STATE_POWER,
            target_temperature: AUTOTUNE_TARGET_TEMPERATURE,
            steady_state_test_time: STEADY_STATE_TEST_TIME,
        }
    }
}

#[derive(Serialize, Deserialize, Copy, Clone)]
pub struct Pump {
    pub pwm_period: Duration,
    pub max_pressure: Bar,
    pub backflush_on_time: Duration,
    pub backflush_off_time: Duration,
}
impl Default for Pump {
    fn default() -> Self {
        const PUMP_PWM_PERIOD: Duration = Duration::from_millis(100);
        const MAX_PUMP_PRESSURE: Bar = 15.0;
        const BACKFLUSH_ON_TIME: Duration = Duration::from_secs(10);
        const BACKFLUSH_OFF_TIME: Duration = Duration::from_secs(10);
        Pump {
            pwm_period: PUMP_PWM_PERIOD,
            max_pressure: MAX_PUMP_PRESSURE,
            backflush_on_time: BACKFLUSH_ON_TIME,
            backflush_off_time: BACKFLUSH_OFF_TIME,
        }
    }
}

#[derive(Serialize, Deserialize, Copy, Clone)]
pub struct LevelSensor {
    pub low_level_threshold: Millimeters,
}
impl Default for LevelSensor {
    fn default() -> Self {
        const LOW_LEVEL_THRESHOLD: Millimeters = 100;
        LevelSensor {
            low_level_threshold: LOW_LEVEL_THRESHOLD,
        }
    }
}

#[derive(Serialize, Deserialize, Copy, Clone)]
pub struct Indicator {
    pub refresh_interval: Duration,
    pub led_count: usize,
}
impl Default for Indicator {
    fn default() -> Self {
        const LED_REFRESH_INTERVAL: Duration = Duration::from_millis(100);
        pub const LED_COUNT: usize = 32;
        Indicator {
            led_count: LED_COUNT,
            refresh_interval: LED_REFRESH_INTERVAL,
        }
    }
}

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
