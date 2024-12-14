use crate::app_state::System;
use crate::sensors::adc::Adc;
use core::borrow::Borrow;
use esp_idf_svc::hal::adc::oneshot::config::AdcChannelConfig;
use esp_idf_svc::hal::{
    adc::oneshot::{AdcChannelDriver, AdcDriver},
    gpio::ADCPin,
};
use std::time::{Duration, Instant};

pub struct BoilerTemperature<'a, T: ADCPin, M: Borrow<AdcDriver<'a, T::Adc>>> {
    adc_driver: AdcChannelDriver<'a, T, M>,
    poll_interval: Duration,
    next_poll: Instant,
    system: System,
    adc_converter: Adc,
}

impl<'a, T, M> BoilerTemperature<'a, T, M>
where
    T: ADCPin,
    M: Borrow<AdcDriver<'a, T::Adc>>,
{
    pub fn new(adc: M, pin: T, poll_interval: Duration, system: System) -> Self {
        let probe_config = AdcChannelConfig::new();
        Self {
            adc_driver: AdcChannelDriver::new(adc, pin, &probe_config).unwrap(),
            poll_interval,
            next_poll: Instant::now(),
            system,
            adc_converter: Adc::new(1024, 3.3),
        }
    }

    pub fn read(&mut self) -> Result<f32, esp_idf_svc::sys::EspError> {
        let raw_adc = self.adc_driver.read()?;
        let voltage = self.adc_converter.raw_to_voltage(raw_adc);
        self.system.set_boiler_temperature(voltage);
        Ok(voltage)
    }

    pub fn poll(&mut self) -> Duration {
        if Instant::now() < self.next_poll {
            return self.next_poll - Instant::now();
        }
        self.read().expect("Failed to read boiler temperature");
        self.next_poll = Instant::now() + self.poll_interval;
        self.poll_interval
    }
}
