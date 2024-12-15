use crate::app_state::System;
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

/// Loadcell struct
pub struct Scale<'a, SckPin, DtPin>
where
    DtPin: Peripheral<P = DtPin> + Pin + InputPin,
    SckPin: Peripheral<P = SckPin> + Pin + OutputPin,
{
    load_sensor: LoadSensor<'a, SckPin, DtPin>,
    poll_interval: Duration,
    next_poll: Instant,
    system: System,
    samples: Vec<f32>,
    samples_to_average: usize,
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
        system: System,
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
            system,
            samples: Vec::new(),
            samples_to_average: samples,
        })
    }

    pub fn is_ready(&self) -> bool {
        self.load_sensor.is_ready()
    }

    pub fn tare(&mut self, times: usize) {
        self.load_sensor.tare(times);
    }

    pub fn poll(&mut self) -> Duration {
        if Instant::now() < self.next_poll {
            return self.next_poll - Instant::now();
        }
        match self.load_sensor.read_scaled() {
            Ok(reading) => {
                if self.samples_to_average > 0 {
                    self.samples.push(reading);
                    if self.samples.len() > self.samples_to_average {
                        let reading = self.samples.iter().sum::<f32>() / self.samples.len() as f32;
                        self.system.set_weight(reading);
                        self.samples.clear();
                    }
                } else {
                    self.system.set_weight(reading);
                }
            }
            Err(e) => {
                log::error!("Failed to read from load sensor: {:?}", e);
            }
        }

        self.next_poll = Instant::now() + self.poll_interval;
        self.poll_interval
    }
}
