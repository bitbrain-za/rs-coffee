use core::borrow::Borrow;
use esp_idf_svc::hal::adc::oneshot::config::AdcChannelConfig;
use esp_idf_svc::hal::{
    adc::oneshot::{AdcChannelDriver, AdcDriver},
    gpio::{ADCPin, Output, OutputPin, PinDriver},
};
use esp_idf_svc::sys::EspError as Error;

pub struct Thermostat<'a, T: ADCPin, M: Borrow<AdcDriver<'a, T::Adc>>, O: OutputPin> {
    adc: AdcChannelDriver<'a, T, M>,
    out: PinDriver<'a, O, Output>,
    target: f32,
}

impl<'a, T, M, O> Thermostat<'a, T, M, O>
where
    T: ADCPin,
    M: Borrow<AdcDriver<'a, T::Adc>>,
    O: OutputPin,
{
    pub fn new(adc: M, adc_pin: T, output_pin: O, target: f32) -> Result<Self, Error> {
        let probe_config = AdcChannelConfig::new();
        Ok(Self {
            adc: AdcChannelDriver::new(adc, adc_pin, &probe_config).unwrap(),
            out: PinDriver::output(output_pin)?,
            target,
        })
    }

    pub fn poll(&mut self) -> Result<(), Error> {
        let temp = self.adc.read()? as f32;
        let temp = Self::convert_raw_to_celsius(temp);

        log::info!("Temperature: {}C", temp);
        if temp > self.target {
            self.turn_off()?;
        } else {
            self.turn_on()?;
        }
        Ok(())
    }

    pub fn turn_on(&mut self) -> Result<(), Error> {
        self.out.set_high()
    }

    pub fn turn_off(&mut self) -> Result<(), Error> {
        self.out.set_low()
    }

    fn convert_raw_to_celsius(raw: f32) -> f32 {
        // Conversion logic goes here
        const BETA: f32 = 3950.0;
        1.0 / ((1.0 / (1023.0 / raw - 1.0)).log(10.0) / BETA + 1.0 / 298.15) - 273.15
    }
}
