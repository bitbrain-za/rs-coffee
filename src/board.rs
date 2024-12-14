use core::borrow::Borrow;
use esp_idf_svc::hal::adc::oneshot::config::AdcChannelConfig;
use esp_idf_svc::hal::{
    adc::oneshot::{AdcChannelDriver, AdcDriver},
    gpio::{ADCPin, Output, OutputPin, PinDriver},
};

use esp_idf_hal::adc::{self, Atten11dB};
use esp_idf_hal::prelude::*;
use esp_idf_hal::units::FromValueType;
use esp_idf_hal::{gpio, gpio::AnyOutputPin, i2c};

pub struct Board {
    pub solenoid_pin: Output,
    pub element_pin: Output,
    pub pump_pin: Output,

    pub adc2: AdcDriver<'static, adc::ADC2>,
    pub adc2_channel_0_temperature: gpio::Gpio11,
}
