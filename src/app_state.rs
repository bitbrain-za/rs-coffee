use crate::board::Board;
use crate::kv_store::Storable;
use crate::models::boiler::BoilerModelParameters;
use crate::system_status::SystemState;
use std::default::Default;
use std::sync::{Arc, Mutex};
pub struct AppState {
    pub system_state: SystemState,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            system_state: SystemState::StartingUp("...".to_string()),
        }
    }
}

impl AppState {
    pub fn update_boiler_model(&mut self, model: BoilerModelParameters) -> Result<(), String> {
        model.save().map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct System<'a> {
    pub app_state: Arc<Mutex<AppState>>,
    pub board: Arc<Mutex<Board<'a>>>,
}

impl<'a> System<'a> {
    pub fn new() -> Self {
        let app_state = AppState::default();
        let app_state = Arc::new(Mutex::new(app_state));
        let board = Board::new();
        let board = Arc::new(Mutex::new(board));

        System { app_state, board }
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
