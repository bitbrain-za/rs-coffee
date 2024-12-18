//! Board module
//!
//! This isn't the actual HW (or should it be?)
//! But essentially, the central point for various threads to set and read HW components

use crate::config;
use crate::gpio::{button::Button, relay::State as RelayState};
use crate::indicator::ring::{Ring, State as IndicatorState};
use crate::sensors::scale::Scale as LoadCell;
use crate::sensors::{pressure::SeeedWaterPressureSensor, pt100::Pt100, traits::TemperatureProbe};
use esp_idf_hal::adc::{
    attenuation,
    oneshot::{config::AdcChannelConfig, AdcChannelDriver, AdcDriver},
};
use esp_idf_hal::gpio::{InterruptType, PinDriver, Pull};
use esp_idf_svc::hal::gpio::{Gpio15, Gpio35, Gpio36, Gpio6, Gpio7, InputPin, OutputPin, Pin};
use esp_idf_svc::hal::peripheral::Peripheral;
use esp_idf_svc::hal::{delay::FreeRtos, prelude::Peripherals};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub enum ButtonEnum {
    Brew,
    Steam,
    HotWater,
}

impl std::fmt::Display for ButtonEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ButtonEnum::Brew => write!(f, "Brew"),
            ButtonEnum::Steam => write!(f, "Steam"),
            ButtonEnum::HotWater => write!(f, "Hot Water"),
        }
    }
}

pub struct Buttons<'a, PA: InputPin + OutputPin, PB: InputPin + OutputPin, PC: InputPin + OutputPin>
{
    brew_button: Button<'a, PA>,
    steam_button: Button<'a, PB>,
    hot_water_button: Button<'a, PC>,
}

impl<'a, PA, PB, PC> Buttons<'a, PA, PB, PC>
where
    PA: InputPin + OutputPin,
    PB: InputPin + OutputPin,
    PC: InputPin + OutputPin,
{
    pub fn was_button_pressed(&mut self, button: ButtonEnum) -> bool {
        match button {
            ButtonEnum::Brew => self.brew_button.was_pressed(),
            ButtonEnum::Steam => self.steam_button.was_pressed(),
            ButtonEnum::HotWater => self.hot_water_button.was_pressed(),
        }
    }

    pub fn button_presses(&mut self) -> Vec<ButtonEnum> {
        let mut buttons = Vec::new();

        if self.was_button_pressed(ButtonEnum::Brew) {
            buttons.push(ButtonEnum::Brew);
        }
        if self.was_button_pressed(ButtonEnum::Steam) {
            buttons.push(ButtonEnum::Steam);
        }
        if self.was_button_pressed(ButtonEnum::HotWater) {
            buttons.push(ButtonEnum::HotWater);
        }

        buttons
    }
}

pub struct Indicators {
    state: Arc<Mutex<IndicatorState>>,
    handle: thread::JoinHandle<()>,
    kill_switch: Arc<Mutex<bool>>,
}

impl Indicators {
    pub fn set_state(&self, state: IndicatorState) {
        *self.state.lock().unwrap() = state;
    }

    pub fn kill(&self) {
        *self.kill_switch.lock().unwrap() = true;
    }
}

pub struct Scale {
    weight: Arc<Mutex<f32>>,
}

impl Scale {
    pub fn get_weight(&self) -> f32 {
        *self.weight.lock().unwrap()
    }
    pub fn set_weight(&self, weight: f32) {
        *self.weight.lock().unwrap() = weight;
    }
}

#[derive(Default)]
pub struct Temperature {
    degrees: Arc<Mutex<f32>>,
}

impl Temperature {
    pub fn get_temperature(&self) -> f32 {
        *self.degrees.lock().unwrap()
    }
    pub fn set_temperature(
        &self,
        voltage: f32,
        probe: impl crate::sensors::traits::TemperatureProbe,
    ) {
        *self.degrees.lock().unwrap() = probe
            .convert_voltage_to_degrees(voltage)
            .expect("Failed to convert voltage to degrees");
    }
}

#[derive(Default)]
pub struct Pressure {
    pressure: f32,
}

impl Pressure {
    pub fn get_pressure(&self) -> f32 {
        self.pressure
    }
    pub fn set_pressure(
        &mut self,
        voltage: f32,
        probe: impl crate::sensors::traits::PressureProbe,
    ) {
        self.pressure = probe
            .convert_voltage_to_pressure(voltage)
            .expect("Failed to convert voltage to pressure");
    }
}

pub struct Sensors<'a> {
    pub buttons: Buttons<'a, Gpio6, Gpio15, Gpio7>,
    pub scale: Scale,
    pub temperature: Arc<Mutex<Temperature>>,
    pub pressure: Arc<Mutex<Pressure>>,
    handle: thread::JoinHandle<()>,
    kill_switch: Arc<Mutex<bool>>,
}

impl Sensors<'_> {
    pub fn kill(&self) {
        *self.kill_switch.lock().unwrap() = true;
    }
}

pub struct Board<'a> {
    pub indicators: Indicators,
    pub sensors: Sensors<'a>,
}

impl<'a> Board<'a> {
    pub fn new() -> Self {
        let peripherals = Peripherals::take().expect("No surprise here :(");

        log::info!("Setting up indicator");
        let led_pin = peripherals.pins.gpio21;
        let channel = peripherals.rmt.channel0;

        let indicator_ring = Arc::new(Mutex::new(IndicatorState::Busy));
        let indicator_killswitch = Arc::new(Mutex::new(false));

        let indicator_ring_clone = indicator_ring.clone();
        let indicator_killswitch_clone = indicator_killswitch.clone();

        let indicator_handle = thread::Builder::new()
            .name("indicator".to_string())
            .spawn(move || {
                let mut ring = Ring::new(
                    channel,
                    led_pin,
                    config::LED_COUNT,
                    config::LED_REFRESH_INTERVAL,
                );
                ring.set_state(IndicatorState::Busy);

                log::info!("Starting indicator thread");
                loop {
                    if *indicator_killswitch_clone.lock().unwrap() {
                        ring.set_state(IndicatorState::Off);
                        log::info!("Indicator thread killed");
                        return;
                    }
                    let requested_indicator_state = *indicator_ring_clone.lock().unwrap();
                    if ring.state != requested_indicator_state {
                        ring.set_state(requested_indicator_state);
                    }
                    thread::sleep(ring.tick());
                }
            })
            .expect("Failed to spawn indicator thread");

        log::info!("Setting up buttons");
        let mut button_brew = Button::new(peripherals.pins.gpio6, None);
        let mut button_steam = Button::new(peripherals.pins.gpio15, None);
        let mut button_hot_water = Button::new(peripherals.pins.gpio7, None);

        button_brew.enable();
        button_steam.enable();
        button_hot_water.enable();

        log::info!("Setting up ADCs");
        let pressure = Arc::new(Mutex::new(Pressure::default()));
        let temperature = Arc::new(Mutex::new(Temperature::default()));
        let pressure_clone = pressure.clone();
        let temperature_clone = temperature.clone();

        use crate::kv_store::Storable;
        let seed_pressure_probe = SeeedWaterPressureSensor::load_or_default();
        let pt100 = Pt100::load_or_default();

        log::info!("Setting up scale");
        let dt = peripherals.pins.gpio36;
        let sck = peripherals.pins.gpio35;
        let weight = Arc::new(Mutex::new(0.0));
        let mut loadcell = LoadCell::new(
            sck,
            dt,
            config::LOAD_SENSOR_SCALING,
            config::SCALE_POLLING_RATE_MS,
            config::SCALE_SAMPLES,
        )
        .unwrap();
        loadcell.tare(32);

        while !loadcell.is_ready() {
            std::thread::sleep(Duration::from_millis(100));
        }

        let sensor_killswitch = Arc::new(Mutex::new(false));
        let sensor_killswitch_clone = sensor_killswitch.clone();
        let weight_clone = weight.clone();
        let sensor_handle = thread::Builder::new()
            .name("sensor".to_string())
            .spawn(move || {
                let adc = AdcDriver::new(peripherals.adc1).expect("Failed to create ADC driver");
                let config = AdcChannelConfig {
                    attenuation: attenuation::DB_11,
                    calibration: true,
                    ..Default::default()
                };

                let temperature_probe =
                    AdcChannelDriver::new(&adc, peripherals.pins.gpio4, &config)
                        .expect("Failed to create ADC channel temperature");
                let pressure_probe = AdcChannelDriver::new(&adc, peripherals.pins.gpio5, &config)
                    .expect("Failed to create ADC channel pressure");
                let mut adc = crate::gpio::adc::Adc::new(
                    temperature_probe,
                    pressure_probe,
                    config::ADC_POLLING_RATE_MS,
                    config::ADC_SAMPLES,
                );

                loop {
                    if *sensor_killswitch_clone.lock().unwrap() {
                        log::info!("Sensor thread killed");
                        return;
                    }

                    if let Some(reading) = loadcell.read() {
                        // [ ] Convert to grams
                        *weight_clone.lock().unwrap() = reading;
                    }

                    if let Some((temperature, pressure)) = adc.read() {
                        temperature_clone
                            .lock()
                            .unwrap()
                            .set_temperature(temperature, pt100);
                        pressure_clone
                            .lock()
                            .unwrap()
                            .set_pressure(pressure, seed_pressure_probe);
                    }

                    FreeRtos::delay_ms(100);
                }
            })
            .expect("Failed to spawn sensor thread");

        Board {
            indicators: Indicators {
                state: indicator_ring,
                handle: indicator_handle,
                kill_switch: indicator_killswitch,
            },
            sensors: Sensors {
                buttons: Buttons {
                    brew_button: button_brew,
                    steam_button: button_steam,
                    hot_water_button: button_hot_water,
                },
                scale: Scale { weight },
                handle: sensor_handle,
                kill_switch: sensor_killswitch,
                temperature,
                pressure,
            },
        }
    }
}
