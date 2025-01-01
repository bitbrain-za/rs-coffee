use core::borrow::Borrow;
use esp_idf_svc::hal::{
    adc::oneshot::{AdcChannelDriver, AdcDriver},
    gpio::ADCPin,
};
use std::time::{Duration, Instant};

enum AdcSimulation {
    Temperature,
    Pressure,
}

pub struct Adc<
    'a,
    T: ADCPin,
    P: ADCPin,
    M: Borrow<AdcDriver<'a, T::Adc>>,
    N: Borrow<AdcDriver<'a, P::Adc>>,
> {
    temperature_probe: AdcChannelDriver<'a, T, M>,
    pressure_probe: AdcChannelDriver<'a, P, N>,
    poll_interval: Duration,
    next_poll: Instant,
    samples: Vec<(u16, u16)>,
    samples_to_average: usize,
    last_reading: (f64, f64),
}

impl<'a, T, P, M, N> Adc<'a, T, P, M, N>
where
    T: ADCPin,
    P: ADCPin,
    M: Borrow<AdcDriver<'a, T::Adc>>,
    N: Borrow<AdcDriver<'a, P::Adc>>,
{
    pub fn new(
        adc1: AdcChannelDriver<'a, T, M>,
        adc2: AdcChannelDriver<'a, P, N>,
        poll_interval: Duration,
        samples: usize,
    ) -> Self {
        Self {
            temperature_probe: adc1,
            pressure_probe: adc2,
            poll_interval,
            next_poll: Instant::now(),
            samples: Vec::new(),
            samples_to_average: samples,
            last_reading: (0.0, 0.0),
        }
    }

    pub fn read(&mut self) -> Option<(f64, f64)> {
        let raw_temperature = self
            .temperature_probe
            .read()
            .expect("Failed to read temperature");
        let raw_pressure = self.pressure_probe.read().expect("Failed to read pressure");

        self.samples.push((raw_temperature, raw_pressure));

        if self.samples.len() > self.samples_to_average {
            let (average_temperature, average_pressure): (u32, u32) = self
                .samples
                .iter()
                .fold((0, 0), |acc, (t, p)| (acc.0 + *t as u32, acc.1 + *p as u32));
            let average_temperature_sample = average_temperature as f64 / self.samples.len() as f64;
            let average_pressure_sample = average_pressure as f64 / self.samples.len() as f64;

            self.samples.clear();

            Some((average_temperature_sample, average_pressure_sample))
        } else {
            None
        }
    }

    pub fn poll(&mut self) -> Duration {
        if Instant::now() < self.next_poll {
            return self.next_poll - Instant::now();
        }
        if let Some((boiler, pressure)) = self.read() {
            self.last_reading = (boiler, pressure);
        }
        self.next_poll = Instant::now() + self.poll_interval - Duration::from_millis(1);
        self.poll_interval
    }
}
