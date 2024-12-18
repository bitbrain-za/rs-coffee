use super::FsmError as Error;
pub enum OperationalState {
    StartingUp(String),
    AutoTuning(std::time::Duration),
    Idle,
    Brewing,
    Steaming,
}

pub enum Transitions {
    Idle,
    StartAutoTune(std::time::Duration),
    AutoTuneComplete,
    StartBrewing,
    StartSteaming,
    Stop,
}

impl Default for OperationalState {
    fn default() -> Self {
        OperationalState::StartingUp("...".to_string())
    }
}

impl OperationalState {
    pub fn transition(&mut self, _next: Transitions) -> Result<(), Error> {
        Err(Error::InvalidStateTransition(
            "Invalid state transition".to_string(),
        ))
    }
}
