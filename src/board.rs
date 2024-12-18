//! Board module
//!
//! This isn't the actual HW (or should it be?)
//! But essentially, the central point for various threads to set and read HW components

use crate::config;
use crate::gpio::{button::Button, relay::State as RelayState};
use crate::indicator::ring::{Ring, State as IndicatorState};
use crate::sensors::{pressure::SeeedWaterPressureSensor, pt100::Pt100, traits::TemperatureProbe};
use esp_idf_svc::hal::gpio::{Gpio15, Gpio6, Gpio7, InputPin, OutputPin};
use esp_idf_svc::hal::prelude::Peripherals;
use std::sync::{Arc, Mutex};
use std::thread;

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
    ring: Arc<Mutex<IndicatorState>>,
    handle: thread::JoinHandle<()>,
    kill_switch: Arc<Mutex<bool>>,
}

impl Indicators {
    pub fn set_state(&self, state: IndicatorState) {
        *self.ring.lock().unwrap() = state;
    }

    pub fn kill(&self) {
        *self.kill_switch.lock().unwrap() = true;
    }
}

pub struct Board<'a> {
    pub indicators: Indicators,
    pub buttons: Buttons<'a, Gpio6, Gpio15, Gpio7>,
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

        Board {
            indicators: Indicators {
                ring: indicator_ring,
                handle: indicator_handle,
                kill_switch: indicator_killswitch,
            },
            buttons: Buttons {
                brew_button: button_brew,
                steam_button: button_steam,
                hot_water_button: button_hot_water,
            },
        }
    }
}
