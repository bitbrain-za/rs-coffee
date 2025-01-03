use crate::board::Board;
use crate::schemas::event::EventBuffer;
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
    pub events: Arc<Mutex<EventBuffer>>,
}

impl System {
    pub fn new() -> Self {
        let operational_state = Arc::new(Mutex::new(OperationalState::default()));
        let board = Board::new(operational_state.clone());

        operational_state
            .transition(Transitions::Idle)
            .expect("Failed to set operational state");

        System {
            system_state: Arc::new(Mutex::new(SystemState::default())),
            operational_state,
            board,
            events: Arc::new(Mutex::new(EventBuffer::new())),
        }
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

    pub fn report_panic_event(&self, source: &str, message: String) {
        let mut event_buffer = self.events.lock().unwrap();
        event_buffer.panic(source, message);
    }

    pub fn report_error_event(&self, source: &str, message: String) {
        let mut event_buffer = self.events.lock().unwrap();
        event_buffer.error(source, message);
    }

    pub fn report_warn_event(&self, source: &str, message: String) {
        let mut event_buffer = self.events.lock().unwrap();
        event_buffer.warn(source, message);
    }

    pub fn report_info_event(&self, source: &str, message: String) {
        let mut event_buffer = self.events.lock().unwrap();
        event_buffer.info(source, message);
    }

    #[allow(dead_code)]
    pub fn report_debug_event(&self, source: &str, message: String) {
        let mut event_buffer = self.events.lock().unwrap();
        event_buffer.debug(source, message);
    }

    #[allow(dead_code)]
    pub fn report_trace_event(&self, source: &str, message: String) {
        let mut event_buffer = self.events.lock().unwrap();
        event_buffer.trace(source, message);
    }
}

#[macro_export]
macro_rules! panic {
    ($self:expr, $($arg:tt)*) => {
        $self.report_panic_event(module_path!(), format!($($arg)*));
    };
}

#[macro_export]
macro_rules! error {
    ($self:expr, $($arg:tt)*) => {
        $self.report_error_event(module_path!(), format!($($arg)*));
    };
}

#[macro_export]
macro_rules! warn {
    ($self:expr, $($arg:tt)*) => {
        $self.report_warn_event(module_path!(), format!($($arg)*));
    };
}

#[macro_export]
macro_rules! info {
    ($self:expr, $($arg:tt)*) => {
        $self.report_info_event(module_path!(), format!($($arg)*));
    };
}

#[macro_export]
macro_rules! debug {
    ($self:expr, $($arg:tt)*) => {
        $self.report_debug_event(module_path!(), format!($($arg)*));
    };
}

#[macro_export]
macro_rules! trace {
    ($self:expr, $($arg:tt)*) => {
        $self.report_trace_event(module_path!(), format!($($arg)*));
    };
}
