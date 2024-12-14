use crate::app_state::System;
use core::borrow::Borrow;
use esp_idf_svc::hal::{
    adc::oneshot::{AdcChannelDriver, AdcDriver},
    gpio::ADCPin,
};
use std::time::{Duration, Instant};

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
    system: System,
    raw_to_vin_factor: f32,
    samples: Vec<(f32, f32)>,
    samples_to_average: usize,
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
        system: System,
    ) -> Self {
        // [ ] these are probably wrong
        const ADC_TOP: f32 = 4096.0;
        const VREF: f32 = 1.1;
        let vin_div_top = VREF / ADC_TOP;

        Self {
            temperature_probe: adc1,
            pressure_probe: adc2,
            poll_interval,
            next_poll: Instant::now(),
            system,
            raw_to_vin_factor: vin_div_top,
            samples: Vec::new(),
            samples_to_average: 100,
        }
    }

    pub fn read(&mut self) {
        let raw_temperature = self
            .temperature_probe
            .read()
            .expect("Failed to read temperature");
        let raw_pressure = self.pressure_probe.read().expect("Failed to read pressure");

        let voltage_temperature = raw_temperature as f32 * self.raw_to_vin_factor;
        let voltage_pressure = raw_pressure as f32 * self.raw_to_vin_factor;
        self.samples.push((voltage_temperature, voltage_pressure));

        if self.samples.len() > self.samples_to_average {
            let (average_temperature, average_pressure) = self
                .samples
                .iter()
                .fold((0.0, 0.0), |acc, (t, p)| (acc.0 + t, acc.1 + p));
            let average_temperature = average_temperature / self.samples.len() as f32;
            let average_pressure = average_pressure / self.samples.len() as f32;
            self.samples.clear();
            self.system.set_boiler_temperature(average_temperature);
            self.system.set_pump_pressure(average_pressure);
        }
    }

    pub fn poll(&mut self) -> Duration {
        if Instant::now() < self.next_poll {
            return self.next_poll - Instant::now();
        }
        self.read();
        self.next_poll = Instant::now() + self.poll_interval;
        self.poll_interval
    }
}
