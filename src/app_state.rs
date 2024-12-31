use crate::board::{self, Board};
use crate::schemas::status::StatusReport;
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
    pub board: Board,
}

impl System {
    pub fn new() -> (Self, board::Element) {
        let operational_state = Arc::new(Mutex::new(OperationalState::default()));
        let (board, element) = Board::new(operational_state.clone());
        let board = board;

        operational_state
            .transition(Transitions::Idle)
            .expect("Failed to set operational state");

        (
            System {
                system_state: Arc::new(Mutex::new(SystemState::default())),
                operational_state,
                board,
            },
            element,
        )
    }

    pub fn generate_report(&self) -> StatusReport {
        let system_state = self.system_state.lock().unwrap().clone();
        let operational_state = self.operational_state.lock().unwrap().clone();
        let board = self.board.generate_report();

        StatusReport {
            status: system_state.to_string(),
            message: None,
            device: board,
            operation: operational_state.to_report(),
        }
    }
}
