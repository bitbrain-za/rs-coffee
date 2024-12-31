use esp_idf_hal::gpio::{PinDriver, Pull};
use esp_idf_svc::hal::gpio::{InputPin, OutputPin};
use std::{
    sync::{Arc, RwLock},
    thread,
};

#[derive(Debug, Default, Copy, Clone, PartialEq)]
enum SwitchState {
    Active,
    #[default]
    Released,
}

impl SwitchState {
    pub fn update(&self, state: bool) -> Option<Self> {
        match (state, self) {
            (true, Self::Released) => Some(Self::Active),
            (false, Self::Active) => Some(Self::Released),
            _ => None,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum SwitchesState {
    Idle,
    Brew,
    HotWater,
    Steam,
    AutoTune,
    Backflush,
}

impl std::fmt::Display for SwitchesState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = match self {
            Self::Idle => "Idle",
            Self::Brew => "Brew",
            Self::HotWater => "Hot Water",
            Self::Steam => "Steam",
            Self::AutoTune => "Autotune",
            Self::Backflush => "Backflush",
        };
        write!(f, "{}", state)
    }
}

impl SwitchesState {
    fn update(&self, brew: SwitchState, hot_water: SwitchState, steam: SwitchState) -> Self {
        match (brew, hot_water, steam) {
            (SwitchState::Active, SwitchState::Active, SwitchState::Active) => Self::AutoTune,
            (SwitchState::Active, SwitchState::Active, SwitchState::Released) => Self::Backflush,
            (SwitchState::Active, _, _) => Self::Brew,
            (_, SwitchState::Active, _) => Self::HotWater,
            (_, _, SwitchState::Active) => Self::Steam,
            _ => Self::Idle,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Switches {
    brew_switch_state: Arc<RwLock<SwitchState>>,
    hot_water_switch_state: Arc<RwLock<SwitchState>>,
    steam_switch_state: Arc<RwLock<SwitchState>>,
}

impl Switches {
    pub fn new<PB, PH, PS>(brew_pin: PB, hot_water_pin: PH, steam_pin: PS) -> Self
    where
        PB: InputPin + OutputPin,
        PH: InputPin + OutputPin,
        PS: InputPin + OutputPin,
    {
        let mut brew_switch_pin =
            PinDriver::input(brew_pin).expect("failed to get switch pin driver");
        let mut hot_water_switch_pin =
            PinDriver::input(hot_water_pin).expect("failed to get hot water switch pin driver");
        let mut steam_switch_pin =
            PinDriver::input(steam_pin).expect("failed to get steam switch pin driver");

        brew_switch_pin
            .set_pull(Pull::Up)
            .expect("failed to configure switch");
        hot_water_switch_pin
            .set_pull(Pull::Up)
            .expect("failed to configure switch");
        steam_switch_pin
            .set_pull(Pull::Up)
            .expect("failed to configure switch");

        let brew_switch_state = Arc::new(RwLock::new(SwitchState::Released));
        let hot_water_switch_state = Arc::new(RwLock::new(SwitchState::Released));
        let steam_switch_state = Arc::new(RwLock::new(SwitchState::Released));

        let brew_clone = brew_switch_state.clone();
        let hot_water_clone = hot_water_switch_state.clone();
        let steam_clone = steam_switch_state.clone();

        std::thread::spawn(move || loop {
            let pin_state = brew_switch_pin.is_low();
            let last_state = *brew_clone.read().unwrap();

            if let Some(new_state) = last_state.update(pin_state) {
                *brew_clone.write().unwrap() = new_state;
            }

            let pin_state = hot_water_switch_pin.is_low();
            let last_state = *hot_water_clone.read().unwrap();
            if let Some(new_state) = last_state.update(pin_state) {
                *hot_water_clone.write().unwrap() = new_state;
            }

            let pin_state = steam_switch_pin.is_low();
            let last_state = *steam_clone.read().unwrap();
            if let Some(new_state) = last_state.update(pin_state) {
                *steam_clone.write().unwrap() = new_state;
            }
            thread::sleep(std::time::Duration::from_millis(100));
        });

        Self {
            brew_switch_state,
            hot_water_switch_state,
            steam_switch_state,
        }
    }

    pub fn get_state(&self) -> SwitchesState {
        let brew = *self.brew_switch_state.read().unwrap();
        let hot_water = *self.hot_water_switch_state.read().unwrap();
        let steam = *self.steam_switch_state.read().unwrap();

        SwitchesState::Idle.update(brew, hot_water, steam)
    }
}
