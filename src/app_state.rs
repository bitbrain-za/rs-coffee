use crate::board::{self, Action, Board, Reading};
use crate::state_machines::{
    operational_fsm::{OperationalState, Transitions},
    system_fsm::SystemState,
    ArcMutexState,
};
use std::default::Default;
use std::sync::{Arc, Mutex};

pub type ApiState = Arc<Mutex<ApiData>>;
pub struct ApiData {
    pub echo_data: String,
    pub drink: Option<crate::schemas::drink::Drink>,
}

#[derive(Clone)]
pub struct System {
    pub system_state: Arc<Mutex<SystemState>>,
    pub operational_state: Arc<Mutex<OperationalState>>,
    pub board: Arc<Mutex<Board<'static>>>,
}

impl System {
    pub fn new() -> (Self, board::Element) {
        let operational_state = Arc::new(Mutex::new(OperationalState::default()));
        let (board, element) = Board::new(operational_state.clone());
        let board = Arc::new(Mutex::new(board));

        // [ ] review this, but for now hit the steam button during startup to initiate auto-tune
        if let Reading::SteamSwitchState(Some(true)) =
            Reading::SteamSwitchState(None).get(board.clone())
        {
            log::info!("Steam button pressed during startup, starting auto-tune");
            operational_state
                .transition(Transitions::StartAutoTune)
                .expect("Failed to set operational state");
        } else {
            operational_state
                .transition(Transitions::Idle)
                .expect("Failed to set operational state");
        }

        (
            System {
                system_state: Arc::new(Mutex::new(SystemState::default())),
                operational_state,
                board,
            },
            element,
        )
    }

    pub fn execute_board_action(&self, action: Action) -> Result<(), String> {
        let system_state = self.system_state.lock().unwrap().clone();

        match (system_state, &action) {
            /* Restrictive States */
            (SystemState::Panic(_), Action::Panic) => {
                action.execute(self.board.clone());
                Ok(())
            }
            (SystemState::Panic(message), _) => Err(message),

            (SystemState::Error(_), Action::Panic) => {
                action.execute(self.board.clone());
                Ok(())
            }
            (SystemState::Error(_), Action::Error) => {
                action.execute(self.board.clone());
                Ok(())
            }
            (SystemState::Error(message), _) => Err(message),

            /* Permissive States */
            (SystemState::Healthy, _) => {
                action.execute(self.board.clone());
                Ok(())
            }
            (SystemState::Warning(message), _) => {
                log::warn!("Executing action while in warning state: {}", message);
                action.execute(self.board.clone());
                Ok(())
            }

            (_, _) => Err("unhandled".to_string()),
        }
    }

    pub fn do_board_read(&self, reading: crate::board::Reading) -> crate::board::Reading {
        reading.get(self.board.clone())
    }

    pub fn read_f32(&self, reading: crate::board::F32Read) -> f32 {
        reading.get(self.board.clone())
    }

    pub fn read_bool(&self, reading: crate::board::BoolRead) -> bool {
        reading.get(self.board.clone())
    }

    pub fn error(&self, message: String) {
        if self.system_state.lock().unwrap().set_error(message).is_ok() {
            if let Err(e) = self.execute_board_action(Action::Error) {
                log::error!("Unable to execute requested action: {}", e);
            }
        }
    }

    pub fn panic(&self, message: String) {
        if self.system_state.lock().unwrap().panic(message).is_ok() {
            if let Err(e) = self.execute_board_action(Action::Panic) {
                log::error!("Unable to execute requested action: {}", e);
            }
        }
    }
}
