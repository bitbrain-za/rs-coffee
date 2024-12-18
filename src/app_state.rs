use crate::board::Board;
use crate::state_machines::{operational_fsm::OperationalState, system_fsm::SystemState};
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
        System {
            system_state: Arc::new(Mutex::new(SystemState::default())),
            operational_state: Arc::new(Mutex::new(OperationalState::default())),
            board: Arc::new(Mutex::new(Board::new())),
        }
    }
}
