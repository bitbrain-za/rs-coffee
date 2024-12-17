use crate::gpio::{button::Button, relay::State as RelayState};
use crate::indicator::ring::State as IndicatorState;
use crate::kv_store::Storable;
use crate::models::boiler::BoilerModelParameters;
use crate::sensors::traits::PressureProbe;
use crate::sensors::{pressure::SeeedWaterPressureSensor, pt100::Pt100, traits::TemperatureProbe};
use crate::system_status::SystemState;
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

#[derive(Debug)]
pub struct AppState {
    pub system_state: SystemState,
    pub indicator_state: IndicatorState,
    pub boiler_state: BoilerState,
    pub solenoid_state: RelayState,
    pub pump_state: PumpState,

    pub weight: f32,

    pub brew_button: Button,
    pub steam_button: Button,
    pub hot_water_button: Button,

    boiler_probe: Pt100,
    pump_probe: SeeedWaterPressureSensor,
}

impl Default for AppState {
    fn default() -> Self {
        log::info!("Setting up NVS");

        AppState {
            system_state: SystemState::StartingUp("...".to_string()),
            indicator_state: IndicatorState::default(),
            boiler_state: BoilerState::default(),
            solenoid_state: RelayState::default(),
            pump_state: PumpState::default(),
            weight: 0.0,
            brew_button: Button::default(),
            steam_button: Button::default(),
            hot_water_button: Button::default(),
            boiler_probe: Pt100::new(),
            pump_probe: SeeedWaterPressureSensor::new(),
        }
    }
}

impl AppState {
    pub fn update_boiler_probe(&mut self, probe: Pt100) -> Result<(), String> {
        probe.save().map_err(|e| e.to_string())?;
        self.boiler_probe = probe;
        Ok(())
    }

    pub fn update_pump_probe(&mut self, probe: SeeedWaterPressureSensor) -> Result<(), String> {
        probe.save().map_err(|e| e.to_string())?;
        self.pump_probe = probe;
        Ok(())
    }

    pub fn update_boiler_model(&mut self, model: BoilerModelParameters) -> Result<(), String> {
        model.save().map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[derive(Debug, Default, Copy, Clone)]
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

#[derive(Debug, Default, Copy, Clone)]
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
        log::info!("App State: {:?}", app_state);
        let app_state = Arc::new(Mutex::new(app_state));
        System { app_state }
    }

    pub fn set_indicator(&self, state: IndicatorState) {
        self.app_state.lock().unwrap().indicator_state = state;
    }

    pub fn get_indicator(&self) -> IndicatorState {
        self.app_state.lock().unwrap().indicator_state
    }

    pub fn set_boiler_temperature(&self, voltage: f32) {
        let temperature = self
            .app_state
            .lock()
            .unwrap()
            .boiler_probe
            .convert_voltage_to_degrees(voltage);
        match temperature {
            Ok(temperature) => {
                self.app_state
                    .lock()
                    .unwrap()
                    .boiler_state
                    .set_temperature(temperature);
            }
            Err(e) => {
                log::error!("Failed to convert temperature: {}", e);
                log::error!("Raw voltage: {}", voltage);
            }
        }
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

    pub fn set_pump_pressure(&self, voltage: f32) {
        let pressure = self
            .app_state
            .lock()
            .unwrap()
            .pump_probe
            .convert_voltage_to_pressure(voltage);
        match pressure {
            Ok(pressure) => {
                self.app_state
                    .lock()
                    .unwrap()
                    .pump_state
                    .set_pressure(pressure);
            }
            Err(e) => {
                log::error!("Failed to convert temperature: {}", e);
                log::error!("Raw voltage: {}", voltage);
            }
        };
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

    pub fn set_weight(&self, weight: f32) {
        self.app_state.lock().unwrap().weight = weight;
    }

    pub fn get_weight(&self) -> f32 {
        self.app_state.lock().unwrap().weight
    }

    pub fn update_pt100(&self, probe: Pt100) -> Result<(), String> {
        self.app_state.lock().unwrap().update_boiler_probe(probe)
    }

    pub fn update_pressure_probe(&self, probe: SeeedWaterPressureSensor) -> Result<(), String> {
        self.app_state.lock().unwrap().update_pump_probe(probe)
    }

    pub fn heating_allowed(&self) -> bool {
        match self.app_state.lock().unwrap().system_state {
            SystemState::StartingUp(_) => false,
            SystemState::Idle => true,
            SystemState::Standby(_) => true,
            SystemState::Heating(_) => true,
            SystemState::Ready => true,
            SystemState::PreInfusing => true,
            SystemState::Brewing => true,
            SystemState::Steaming => true,
            SystemState::HotWater => true,
            SystemState::Cleaning => true,
            SystemState::Error(_) => false,
            SystemState::Panic(_) => false,
        }
    }
}
