use super::traits::*;
use super::FsmError as Error;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub enum OperationalState {
    StartingUp(String),
    AutoTuning(Duration, Instant),
    Idle,
    Brewing,
    Steaming,
}

impl OperationalState {
    pub fn time_remaining(&self) -> Option<Duration> {
        match self {
            Self::AutoTuning(length, start) => Some(*length - start.elapsed()),
            _ => None,
        }
    }

    pub fn percentage_complete(&self) -> Option<f32> {
        match self {
            Self::AutoTuning(length, start) => {
                Some(start.elapsed().as_secs_f32() / length.as_secs_f32() * 100.0)
            }
            _ => None,
        }
    }
}

impl std::fmt::Display for OperationalState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationalState::StartingUp(stage) => write!(f, "Starting up: {}", stage),
            OperationalState::AutoTuning(d, i) => {
                write!(
                    f,
                    "Auto-tuning for {}s started {}s ago",
                    d.as_secs(),
                    i.elapsed().as_secs()
                )
            }
            OperationalState::Idle => write!(f, "Idle"),
            OperationalState::Brewing => write!(f, "Brewing"),
            OperationalState::Steaming => write!(f, "Steaming"),
        }
    }
}

pub enum Transitions {
    StartingUpStage(String),
    Idle,
    StartAutoTune(Duration),
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
                *self = OperationalState::AutoTuning(*d, Instant::now());
                Ok(())
            }
            (_, Transitions::StartAutoTune(_)) => Err(Error::InvalidStateTransition(
                "Cannot start auto-tune from current state".to_string(),
            )),

            (OperationalState::StartingUp(_), _) => {
                Err(Error::Busy("System is still starting up".to_string(), None))
            }
            (OperationalState::AutoTuning(_, _), Transitions::AutoTuneComplete) => {
                *self = OperationalState::Idle;
                Ok(())
            }
            (OperationalState::AutoTuning(length, start), _) => {
                let remaining = *length - start.elapsed();
                Err(Error::Busy(
                    "System is still busy auto-tuning".to_string(),
                    Some(remaining),
                ))
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
