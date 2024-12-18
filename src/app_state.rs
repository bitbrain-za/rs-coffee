use crate::board::{Board, Reading};
use crate::state_machines::{
    operational_fsm::{OperationalState, Transitions},
    system_fsm::SystemState,
    ArcMutexState,
};
use std::default::Default;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct System<'a> {
    pub system_state: Arc<Mutex<SystemState>>,
    pub operational_state: Arc<Mutex<OperationalState>>,
    pub board: Arc<Mutex<Board<'a>>>,
}

impl<'a> System<'a> {
    pub fn new() -> Self {
        let operational_state = Arc::new(Mutex::new(OperationalState::default()));
        let board = Arc::new(Mutex::new(Board::new(operational_state.clone())));

        // [ ] review this, but for now hit the steam button during startup to initiate auto-tune
        if let Reading::SteamSwitchState(Some(true)) =
            Reading::SteamSwitchState(None).get(board.clone())
        {
            log::info!("Steam button pressed during startup, starting auto-tune");
            operational_state
                .transition(Transitions::StartAutoTune(std::time::Duration::from_secs(
                    30 * 60,
                )))
                .expect("Failed to set operational state");
        } else {
            operational_state
                .transition(Transitions::Idle)
                .expect("Failed to set operational state");
        }

        System {
            system_state: Arc::new(Mutex::new(SystemState::default())),
            operational_state,
            board,
        }
    }

    pub fn execute_board_action(&self, action: crate::board::Action) {
        action.execute(self.board.clone());
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
}
