use super::traits::*;
use super::FsmError as Error;
use std::sync::{Arc, Mutex};

pub enum OperationalState {
    StartingUp(String),
    AutoTuning(std::time::Duration),
    Idle,
    Brewing,
    Steaming,
}

pub enum Transitions {
    StartingUpStage(String),
    Idle,
    StartAutoTune(std::time::Duration),
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
            (OperationalState::StartingUp(_), Transitions::StartAutoTune(d)) => {
                log::info!("Starting auto-tune");
                *self = OperationalState::AutoTuning(*d);
                Ok(())
            }
            (_, Transitions::StartAutoTune(_)) => Err(Error::InvalidStateTransition(
                "Cannot start auto-tune from current state".to_string(),
            )),

            (OperationalState::StartingUp(_), _) => {
                Err(Error::Busy("System is still starting up".to_string()))
            }
            (OperationalState::AutoTuning(_), Transitions::AutoTuneComplete) => {
                *self = OperationalState::Idle;
                Ok(())
            }
            (OperationalState::AutoTuning(_), _) => {
                Err(Error::Busy("System is still busy auto-tuning".to_string()))
            }

            (_, _) => Err(Error::NotYetImplemented),
        }
    }
}

impl ArcMutexState<Transitions> for Arc<Mutex<OperationalState>> {
    fn transition(&self, next: Transitions) -> Result<(), Error> {
        let mut state = self.lock().unwrap();
        state.transition(next)
    }
}
