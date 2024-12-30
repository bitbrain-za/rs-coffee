use crate::{
    config,
    kv_store::{Error as KvsError, Key, KeyValueStore, Storable, Value},
    schemas::types::Grams,
};
use anyhow::Result;
use esp_idf_svc::hal::{
    delay::Ets,
    gpio::{Input, InputPin, Output, OutputPin, Pin, PinDriver},
    peripheral::Peripheral,
};
use loadcell::{hx711::HX711, LoadCell};
use std::sync::{
    mpsc::{channel, Sender},
    Arc, RwLock,
};
use std::time::{Duration, Instant};

pub type LoadSensor<'a, SckPin, DtPin> =
    HX711<PinDriver<'a, SckPin, Output>, PinDriver<'a, DtPin, Input>, Ets>;

#[derive(Debug, Copy, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScaleConfig {
    pub scaling: f32,
}
impl Default for ScaleConfig {
    fn default() -> Self {
        Self {
            scaling: config::LOAD_SENSOR_SCALING,
        }
    }
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

pub enum Message {
    Tare(usize),
    Scale(f32),
    SetPollInterval(Duration),
    SetFilterWindow(usize),
}

#[derive(Clone)]
pub struct Interface {
    pub mailbox: Sender<Message>,
    pub weight: Arc<RwLock<Grams>>,
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
    interface: Interface,
}

impl<'a, SckPin, DtPin> Scale<'a, SckPin, DtPin>
where
    DtPin: Peripheral<P = DtPin> + Pin + InputPin,
    SckPin: Peripheral<P = SckPin> + Pin + OutputPin,
{
    fn is_ready(&self) -> bool {
        self.load_sensor.is_ready()
    }

    fn tare(&mut self, times: usize) {
        self.load_sensor.tare(times);
    }

    fn read(&mut self) -> Option<f32> {
        if let Ok(reading) = self.load_sensor.read_scaled() {
            self.samples.push(reading);
            if self.samples.len() > self.samples_to_average {
                self.samples
                    .drain(0..(self.samples.len() - self.samples_to_average));
            }
        }
        if self.samples.is_empty() {
            None
        } else {
            Some(self.samples.iter().sum::<f32>() / self.samples.len() as f32)
        }
    }

    fn poll(&mut self) -> Duration {
        if Instant::now() < self.next_poll {
            return self.next_poll - Instant::now();
        }

        if let Some(reading) = self.read() {
            *self.interface.weight.write().unwrap() = reading;
        }

        self.next_poll = Instant::now() + self.poll_interval;
        self.poll_interval
    }

    pub fn start(
        clock_pin: SckPin,
        data_pin: DtPin,
        poll_interval: Duration,
        samples: usize,
    ) -> Result<Interface> {
        let dt = PinDriver::input(data_pin)?;
        let sck = PinDriver::output(clock_pin)?;
        let mut load_sensor = HX711::new(sck, dt, Ets);

        let scaling = ScaleConfig::load_or_default().scaling;

        let (tx, rx) = channel();

        let interface = Interface {
            mailbox: tx,
            weight: Arc::new(RwLock::new(0.0)),
        };

        load_sensor.set_scale(scaling);

        let loadcell = Scale {
            load_sensor,
            poll_interval,
            next_poll: Instant::now(),
            samples: Vec::new(),
            samples_to_average: samples,
            interface: interface.clone(),
        };

        std::thread::Builder::new()
            .name("Scale".to_string())
            .spawn(move || {
                let mut loadcell = loadcell;

                while loadcell.is_ready() {
                    std::thread::sleep(poll_interval);
                }
                loadcell.tare(samples);
                loop {
                    while let Ok(message) = rx.try_recv() {
                        match message {
                            Message::Tare(times) => {
                                loadcell.samples.clear();
                                loadcell.tare(times);
                            }
                            Message::Scale(scaling) => {
                                loadcell.samples.clear();
                                loadcell.load_sensor.set_scale(scaling);
                                let _ = ScaleConfig { scaling }.save();
                            }
                            Message::SetPollInterval(duration) => {
                                loadcell.poll_interval = duration;
                                loadcell.next_poll = Instant::now();
                            }
                            Message::SetFilterWindow(samples) => {
                                loadcell.samples_to_average = samples;
                            }
                        }
                    }

                    std::thread::sleep(loadcell.poll());
                }
            })
            .unwrap();

        Ok(interface)
    }
}
