use core::borrow::Borrow;
use esp_idf_svc::hal::adc::oneshot::config::AdcChannelConfig;
use esp_idf_svc::hal::{
    adc::oneshot::{AdcChannelDriver, AdcDriver},
    gpio::{ADCPin, Output, OutputPin, PinDriver},
};

pub struct Solenoid<'a, PD: OutputPin> {
    out: PinDriver<'a, PD, Output>,
}

impl<'a, PD> Solenoid<'a, PD>
where
    PD: OutputPin,
{
    pub fn new(pin: PD) -> Result<Self, esp_idf_svc::sys::EspError> {
        let out = PinDriver::output(pin)?;
        Ok(Self { out })
    }

    pub fn set_high(&mut self) -> Result<(), esp_idf_svc::sys::EspError> {
        self.out.set_high()
    }

    pub fn set_low(&mut self) -> Result<(), esp_idf_svc::sys::EspError> {
        self.out.set_low()
    }
}

pub struct Ntc<'a, T: ADCPin, M: Borrow<AdcDriver<'a, T::Adc>>> {
    adc: AdcChannelDriver<'a, T, M>,
}

impl<'a, T, M> Ntc<'a, T, M>
where
    T: ADCPin,
    M: Borrow<AdcDriver<'a, T::Adc>>,
{
    pub fn new(adc: M, pin: T) -> Self {
        let probe_config = AdcChannelConfig::new();
        Self {
            adc: AdcChannelDriver::new(adc, pin, &probe_config).unwrap(),
        }
    }

    pub fn read(&mut self) -> Result<u16, esp_idf_svc::sys::EspError> {
        self.adc.read()
    }
}
