use crate::app_state::System;
use crate::sensors::{pt100::Pt100, traits::TemperatureProbe};
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
    system: System,
    raw_to_vin_factor: f32,
    samples: Vec<(u16, u16)>,
    samples_to_average: usize,
    boiler_probe: Pt100,
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

        let boiler_probe = Pt100::new(None);

        Self {
            temperature_probe: adc1,
            pressure_probe: adc2,
            poll_interval,
            next_poll: Instant::now(),
            system,
            raw_to_vin_factor: vin_div_top,
            samples: Vec::new(),
            samples_to_average: 100,
            boiler_probe,
        }
    }

    fn raw_to_voltage(&self, raw: f32, sim: AdcSimulation) -> f32 {
        // raw as f32 * self.raw_to_vin_factor
        match sim {
            AdcSimulation::Temperature => {
                let mut voltage = raw / 5090.0 * 2.71;
                voltage += 1.884;
                voltage
            }
            AdcSimulation::Pressure => raw * self.raw_to_vin_factor,
        }
    }

    pub fn read(&mut self) {
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
            let average_temperature_sample = average_temperature as f32 / self.samples.len() as f32;
            let average_pressure_sample = average_pressure as f32 / self.samples.len() as f32;
            let average_temperature =
                self.raw_to_voltage(average_temperature_sample, AdcSimulation::Temperature);
            let average_pressure =
                self.raw_to_voltage(average_pressure_sample, AdcSimulation::Pressure);

            self.samples.clear();

            self.system.set_pump_pressure(average_pressure);

            match self
                .boiler_probe
                .convert_voltage_to_degrees(average_temperature)
            {
                Ok(temperature) => {
                    self.system.set_boiler_temperature(temperature);
                }
                Err(e) => {
                    log::error!("Failed to convert temperature: {}", e);
                    log::error!("Raw voltage: {}", average_temperature);
                }
            }
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
