use crate::indicator::ring::State as IndicatorState;
pub struct AppState {
    pub indicator_state: IndicatorState,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            indicator_state: IndicatorState::Off,
        }
    }
}
