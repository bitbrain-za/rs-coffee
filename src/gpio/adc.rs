use crate::app_state::System;
use core::borrow::Borrow;
use esp_idf_svc::hal::adc::oneshot::config::AdcChannelConfig;
use esp_idf_svc::hal::adc::{AdcContConfig, AdcContDriver, AdcMeasurement, Attenuated};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::{
    adc::oneshot::{AdcChannelDriver, AdcDriver},
    gpio::ADCPin,
};
use std::fmt::Result;

pub struct Adc<'a> {
    system: System,
    adc: AdcContDriver<'a>,
}

impl<'a> Adc<'a> {
    pub fn new(system: System, adc: AdcContDriver<'a>) -> Self {
        Self { system, adc }
    }

    pub fn start(mut self) {
        self.adc.start().expect("Failed to start ADC driver");

        // Default to just read 10 measurements per each read
        let mut samples: [AdcMeasurement; 10] = [Default::default(); 10];

        loop {
            let result = self.adc.read(&mut samples, 1000);
            if let Ok(num_read) = result {
                log::info!("Read {} measurement.", num_read);
                let average =
                    samples.iter().map(|m| m.data() as f32).sum::<f32>() / num_read as f32;

                self.system.set_boiler_temperature(average);

                log::info!("Average: {}", average);
            } else {
                log::error!("Failed to read ADC measurements: {:?}", result);
            }
        }
    }
}
