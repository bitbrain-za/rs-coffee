use crate::gpio::{button::Button, relay::State as RelayState};
use crate::indicator::ring::State as IndicatorState;
use std::default::Default;
use std::sync::{Arc, Mutex};

pub enum Buttons {
    Brew,
    Steam,
    HotWater,
}

impl std::fmt::Display for Buttons {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Buttons::Brew => write!(f, "Brew"),
            Buttons::Steam => write!(f, "Steam"),
            Buttons::HotWater => write!(f, "Hot Water"),
        }
    }
}

#[derive(Default)]
pub struct AppState {
    pub indicator_state: IndicatorState,
    pub boiler_state: BoilerState,
    pub solenoid_state: RelayState,
    pub pump_state: PumpState,

    pub brew_button: Button,
    pub steam_button: Button,
    pub hot_water_button: Button,
}

#[derive(Default, Copy, Clone)]
pub struct BoilerState {
    duty_cycle: f32,
    temperature: f32,
}

impl BoilerState {
    pub fn set_duty_cycle(&mut self, duty_cycle: f32) {
        self.duty_cycle = duty_cycle;
    }

    pub fn get_duty_cycle(&self) -> f32 {
        self.duty_cycle
    }

    pub fn set_temperature(&mut self, temperature: f32) {
        self.temperature = temperature;
    }

    pub fn get_temperature(&self) -> f32 {
        self.temperature
    }
}

#[derive(Default, Copy, Clone)]
pub struct PumpState {
    duty_cycle: f32,
    pressure: f32,
}

impl PumpState {
    pub fn set_duty_cycle(&mut self, duty_cycle: f32) {
        self.duty_cycle = duty_cycle;
    }

    pub fn get_duty_cycle(&self) -> f32 {
        self.duty_cycle
    }

    pub fn set_pressure(&mut self, pressure: f32) {
        self.pressure = pressure;
    }

    pub fn get_pressure(&self) -> f32 {
        self.pressure
    }
}

#[derive(Default, Clone)]
pub struct System {
    app_state: Arc<Mutex<AppState>>,
}

impl System {
    pub fn new() -> Self {
        let app_state = AppState::default();
        let app_state = Arc::new(Mutex::new(app_state));
        System { app_state }
    }

    pub fn set_indicator(&self, state: IndicatorState) {
        self.app_state.lock().unwrap().indicator_state = state;
    }

    pub fn get_indicator(&self) -> IndicatorState {
        self.app_state.lock().unwrap().indicator_state
    }

    pub fn set_boiler_temperature(&self, temperature: f32) {
        self.app_state
            .lock()
            .unwrap()
            .boiler_state
            .set_temperature(temperature);
    }

    pub fn get_boiler_temperature(&self) -> f32 {
        self.app_state
            .lock()
            .unwrap()
            .boiler_state
            .get_temperature()
    }

    pub fn set_boiler_duty_cycle(&self, duty_cycle: f32) {
        self.app_state
            .lock()
            .unwrap()
            .boiler_state
            .set_duty_cycle(duty_cycle);
    }

    pub fn get_boiler_duty_cycle(&self) -> f32 {
        self.app_state.lock().unwrap().boiler_state.get_duty_cycle()
    }

    pub fn set_pump_pressure(&self, pressure: f32) {
        self.app_state
            .lock()
            .unwrap()
            .pump_state
            .set_pressure(pressure);
    }

    pub fn get_pump_pressure(&self) -> f32 {
        self.app_state.lock().unwrap().pump_state.get_pressure()
    }

    pub fn set_pump_duty_cycle(&self, duty_cycle: f32) {
        self.app_state
            .lock()
            .unwrap()
            .pump_state
            .set_duty_cycle(duty_cycle);
    }

    pub fn get_pump_duty_cycle(&self) -> f32 {
        self.app_state.lock().unwrap().pump_state.get_duty_cycle()
    }

    pub fn solenoid_turn_on(&self, on_time: Option<std::time::Duration>) {
        self.app_state.lock().unwrap().solenoid_state = RelayState::on(on_time);
    }

    #[allow(dead_code)]
    pub fn solenoid_turn_off(&mut self, off_time: Option<std::time::Duration>) {
        self.app_state.lock().unwrap().solenoid_state = RelayState::off(off_time);
    }

    pub fn get_solenoid_state(&self) -> RelayState {
        self.app_state.lock().unwrap().solenoid_state
    }

    pub fn press_button(&self, button: Buttons) {
        match button {
            Buttons::Brew => self.app_state.lock().unwrap().brew_button.press(),
            Buttons::Steam => self.app_state.lock().unwrap().steam_button.press(),
            Buttons::HotWater => self.app_state.lock().unwrap().hot_water_button.press(),
        }
    }

    pub fn was_button_pressed(&self, button: Buttons) -> bool {
        match button {
            Buttons::Brew => self.app_state.lock().unwrap().brew_button.was_pressed(),
            Buttons::Steam => self.app_state.lock().unwrap().steam_button.was_pressed(),
            Buttons::HotWater => self
                .app_state
                .lock()
                .unwrap()
                .hot_water_button
                .was_pressed(),
        }
    }

    pub fn button_presses(&self) -> Vec<Buttons> {
        let mut buttons = Vec::new();

        if self.was_button_pressed(Buttons::Brew) {
            buttons.push(Buttons::Brew);
        }
        if self.was_button_pressed(Buttons::Steam) {
            buttons.push(Buttons::Steam);
        }
        if self.was_button_pressed(Buttons::HotWater) {
            buttons.push(Buttons::HotWater);
        }

        buttons
    }
}
