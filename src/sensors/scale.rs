use crate::{config::LoadCell as Config, types::Grams};
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
    pub flow: Arc<RwLock<f32>>,
}

impl Interface {
    pub fn get_weight(&self) -> f32 {
        *self.weight.read().unwrap()
    }

    pub fn get_flow(&self) -> f32 {
        *self.flow.read().unwrap()
    }

    pub fn tare(&self, times: usize) {
        let _ = self.mailbox.send(Message::Tare(times));
    }

    pub fn set_scaling(&self, scaling: f32) {
        let _ = self.mailbox.send(Message::Scale(scaling));
    }

    pub fn set_poll_interval(&self, duration: Duration) {
        let _ = self.mailbox.send(Message::SetPollInterval(duration));
    }

    pub fn set_filter_window(&self, samples: usize) {
        let _ = self.mailbox.send(Message::SetFilterWindow(samples));
    }

    pub fn start_brew(&self) {
        self.set_filter_window(10);
        self.set_poll_interval(Duration::from_millis(50));
        self.tare(32)
    }

    pub fn stop_brewing(&self) {
        self.set_filter_window(8);
        self.set_poll_interval(Duration::from_millis(250));
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
    samples: Vec<(Instant, f32)>,
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
            self.samples.push((Instant::now(), reading));
            if self.samples.len() > self.samples_to_average {
                self.samples
                    .drain(0..(self.samples.len() - self.samples_to_average));
            }
        }
        if self.samples.is_empty() {
            None
        } else {
            Some(self.samples.iter().map(|(_, m)| m).sum::<f32>() / self.samples.len() as f32)
        }
    }

    fn estimate_flow(&self) {
        let samples = &self.samples;
        if samples.len() < self.samples_to_average {
            *self.interface.flow.write().unwrap() = 0.0;
        }

        let (first, last) = (samples.first().unwrap(), samples.last().unwrap());
        let time = last.0 - first.0;
        let weight = last.1 - first.1;

        *self.interface.flow.write().unwrap() = weight / time.as_secs_f32();
    }

    fn poll(&mut self) -> Duration {
        if Instant::now() < self.next_poll {
            return self.next_poll - Instant::now();
        }

        if let Some(reading) = self.read() {
            *self.interface.weight.write().unwrap() = reading;
            self.estimate_flow();
        }

        self.next_poll = Instant::now() + self.poll_interval;
        self.poll_interval
    }

    pub fn start(clock_pin: SckPin, data_pin: DtPin, config: &Config) -> Result<Interface> {
        let dt = PinDriver::input(data_pin)?;
        let sck = PinDriver::output(clock_pin)?;
        let mut load_sensor = HX711::new(sck, dt, Ets);

        let (tx, rx) = channel();

        let interface = Interface {
            mailbox: tx,
            weight: Arc::new(RwLock::new(0.0)),
            flow: Arc::new(RwLock::new(0.0)),
        };

        load_sensor.set_scale(config.scaling);

        let loadcell = Scale {
            load_sensor,
            poll_interval: config.sampling_rate,
            next_poll: Instant::now(),
            samples: Vec::new(),
            samples_to_average: config.window,
            interface: interface.clone(),
        };

        std::thread::Builder::new()
            .name("Scale".to_string())
            .spawn(move || {
                let mut loadcell = loadcell;

                while loadcell.is_ready() {
                    std::thread::sleep(loadcell.poll_interval);
                }
                loadcell.tare(32);
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
