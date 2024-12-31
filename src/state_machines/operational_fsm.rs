use super::traits::*;
use super::FsmError as Error;
use crate::schemas::status::Operation as OperationReport;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub enum OperationalState {
    StartingUp(String),
    AutoTuneInit,
    AutoTuning,
    Idle,
    Brewing,
    Steaming,
}

impl std::fmt::Display for OperationalState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationalState::StartingUp(stage) => write!(f, "Starting up: {}", stage),
            OperationalState::AutoTuning => {
                write!(f, "Auto-tuning")
            }
            OperationalState::AutoTuneInit => write!(f, "Initialising auto-tune"),
            OperationalState::Idle => write!(f, "Idle"),
            OperationalState::Brewing => write!(f, "Brewing"),
            OperationalState::Steaming => write!(f, "Steaming"),
        }
    }
}

pub enum Transitions {
    StartingUpStage(String),
    Idle,
    StartAutoTune,
    AutoTuneComplete,
    StartBrewing,
    StartSteaming,
    Stop,
}

impl super::traits::StateTrasition for Transitions {}

impl Default for OperationalState {
    fn default() -> Self {
        OperationalState::StartingUp("...".to_string())
    }
}

impl OperationalState {
    pub fn transition(&mut self, next: Transitions) -> Result<(), Error> {
        match (&self, &next) {
            (OperationalState::StartingUp(_), Transitions::Idle) => {
                *self = OperationalState::Idle;
                Ok(())
            }
            (OperationalState::StartingUp(current), Transitions::StartingUpStage(message)) => {
                log::info!("Moving from Stage: {} to Stage: {}", current, message);
                *self = OperationalState::StartingUp(message.clone());
                Ok(())
            }
            (OperationalState::StartingUp(_), Transitions::StartAutoTune) => {
                log::info!("Starting auto-tune");
                *self = OperationalState::AutoTuneInit;
                Ok(())
            }
            (OperationalState::AutoTuneInit, Transitions::StartAutoTune) => {
                *self = OperationalState::AutoTuning;
                Ok(())
            }
            (_, Transitions::StartAutoTune) => Err(Error::InvalidStateTransition(
                "Cannot start auto-tune from current state".to_string(),
            )),
            (OperationalState::StartingUp(_), _) => {
                Err(Error::Busy("System is still starting up".to_string(), None))
            }
            (OperationalState::AutoTuning, Transitions::AutoTuneComplete) => {
                *self = OperationalState::Idle;
                Ok(())
            }
            (OperationalState::AutoTuning, _) => Err(Error::Busy(
                "System is still busy auto-tuning".to_string(),
                None,
            )),

            (_, _) => Err(Error::NotYetImplemented),
        }
    }

    pub fn to_report(&self) -> OperationReport {
        OperationReport {
            state: self.to_string(),
            attributes: None,
        }
    }
}

impl ArcMutexState<Transitions> for Arc<Mutex<OperationalState>> {
    fn transition(&self, next: Transitions) -> Result<(), Error> {
        let mut state = self.lock().unwrap();
        state.transition(next)
    }
}
