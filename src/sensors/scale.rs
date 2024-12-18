use crate::kv_store::{Error as KvsError, Key, KeyValueStore, Storable, Value};
use anyhow::Result;
use esp_idf_svc::hal::{
    delay::Ets,
    gpio::{Input, InputPin, Output, OutputPin, Pin, PinDriver},
    peripheral::Peripheral,
};
use loadcell::{hx711::HX711, LoadCell};
use std::time::{Duration, Instant};

pub type LoadSensor<'a, SckPin, DtPin> =
    HX711<PinDriver<'a, SckPin, Output>, PinDriver<'a, DtPin, Input>, Ets>;

#[derive(Debug, Default, Copy, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScaleConfig {
    pub offset: f32,
}

impl From<&ScaleConfig> for Value {
    fn from(params: &ScaleConfig) -> Self {
        Value::ScaleParameters(*params)
    }
}

impl Storable for ScaleConfig {
    fn load_or_default() -> ScaleConfig {
        let kvs = match KeyValueStore::new_blocking(std::time::Duration::from_millis(1000)) {
            Ok(kvs) => kvs,
            Err(e) => {
                log::error!("Failed to create key value store: {:?}", e);
                return Self::default();
            }
        };
        match kvs.get(Key::ScaleParameters) {
            Value::ScaleParameters(calibration) => calibration,
            _ => Self::default(),
        }
    }

    fn save(&self) -> Result<(), KvsError> {
        let mut kvs = KeyValueStore::new_blocking(std::time::Duration::from_millis(1000))?;
        kvs.set(Value::from(self))
    }
}

pub struct Scale<'a, SckPin, DtPin>
where
    DtPin: Peripheral<P = DtPin> + Pin + InputPin,
    SckPin: Peripheral<P = SckPin> + Pin + OutputPin,
{
    load_sensor: LoadSensor<'a, SckPin, DtPin>,
    poll_interval: Duration,
    next_poll: Instant,
    samples: Vec<f32>,
    samples_to_average: usize,
    last_reading: f32,
}

impl<'a, SckPin, DtPin> Scale<'a, SckPin, DtPin>
where
    DtPin: Peripheral<P = DtPin> + Pin + InputPin,
    SckPin: Peripheral<P = SckPin> + Pin + OutputPin,
{
    pub fn new(
        clock_pin: SckPin,
        data_pin: DtPin,
        scaling: f32,
        poll_interval: Duration,
        samples: usize,
    ) -> Result<Self> {
        let dt = PinDriver::input(data_pin)?;
        let sck = PinDriver::output(clock_pin)?;
        let mut load_sensor = HX711::new(sck, dt, Ets);

        load_sensor.set_scale(scaling);

        Ok(Scale {
            load_sensor,
            poll_interval,
            next_poll: Instant::now(),
            samples: Vec::new(),
            samples_to_average: samples,
            last_reading: 0.0,
        })
    }

    pub fn is_ready(&self) -> bool {
        self.load_sensor.is_ready()
    }

    pub fn tare(&mut self, times: usize) {
        self.load_sensor.tare(times);
    }

    pub fn read(&mut self) -> Option<f32> {
        match self.load_sensor.read_scaled() {
            Ok(reading) => {
                if self.samples_to_average > 0 {
                    self.samples.push(reading);
                    if self.samples.len() > self.samples_to_average {
                        let reading = self.samples.iter().sum::<f32>() / self.samples.len() as f32;
                        self.samples.clear();
                        Some(reading)
                    } else {
                        None
                    }
                } else {
                    Some(reading)
                }
            }
            Err(e) => {
                log::error!("Failed to read from load sensor: {:?}", e);
                // [ ] add an error state
                None
            }
        }
    }

    pub fn poll(&mut self) -> Duration {
        if Instant::now() < self.next_poll {
            return self.next_poll - Instant::now();
        }

        if let Some(reading) = self.read() {
            self.last_reading = reading;
        }

        self.next_poll = Instant::now() + self.poll_interval - Duration::from_millis(1);
        self.poll_interval
    }
}
