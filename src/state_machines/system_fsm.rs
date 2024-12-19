use super::FsmError as Error;

#[derive(Debug, Clone)]
pub enum SystemState {
    StartingUp(String),
    Healthy,
    Warning(String),
    Error(String),
    Panic(String),
}

pub enum Transition {
    Idle,
    Warning(String),
    ClearWarnings,
    Error(String),
    ClearErrros,
    Panic(String),
}

impl Default for SystemState {
    fn default() -> Self {
        SystemState::StartingUp("Created App State".to_string())
    }
}

impl std::fmt::Display for SystemState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SystemState::StartingUp(message) => write!(f, "StartingUp: {}", message),
            SystemState::Healthy => write!(f, "Healthy"),
            SystemState::Warning(message) => write!(f, "Warning: {}", message),
            SystemState::Error(message) => write!(f, "Error: {}", message),
            SystemState::Panic(message) => write!(f, "Panic: {}", message),
        }
    }
}

impl std::fmt::Display for Transition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Transition::Idle => write!(f, "Return to idle"),
            Transition::Warning(message) => write!(f, "Setting warning: {}", message),
            Transition::ClearWarnings => write!(f, "Clear Warnings"),
            Transition::Error(message) => write!(f, "Error: {}", message),
            Transition::ClearErrros => write!(f, "Clear Errors"),
            Transition::Panic(message) => write!(f, "Panic: {}", message),
        }
    }
}

impl SystemState {
    pub fn transition(&mut self, next: Transition) -> Result<(), Error> {
        let result = match (&self, &next) {
            /* ---------------------- */
            /* --- Panic Handling --- */
            /* ---------------------- */

            /* We are in a panic, and a new panic comes along */
            (SystemState::Panic(current), Transition::Panic(message)) => {
                let message = format!("{} | {}", current, message);
                Ok(SystemState::Panic(message))
            }

            /* We are in a panic and something non panic comes along => The only way out of a panic is to reboot*/
            (SystemState::Panic(_), _) => Err(Error::SystemInErrorState(self.to_string())),

            /* We are not in a panic and a panic comes */
            (_, Transition::Panic(message)) => Ok(SystemState::Panic(message.clone())),

            /* ---------------------- */
            /* --- Error Handling --- */
            /* ---------------------- */

            /* We are in an error state and a new error comes along */
            (SystemState::Error(current), Transition::Error(message)) => {
                let message = format!("{} | {}", current, message);
                Ok(SystemState::Error(message))
            }

            /* We are in an error state and new state comes our way (not panic, not error) */
            (SystemState::Error(_), Transition::ClearErrros) => Ok(SystemState::Healthy),

            /* We are in an error state and new state comes our way (not panic, not error) */
            (SystemState::Error(_), _) => Err(Error::SystemInErrorState(self.to_string())),

            /* We are not in a error or panic state and error comes */
            (_, Transition::Error(message)) => Ok(SystemState::Error(message.clone())),

            /* --------------------------- */
            /* --- Startup Transitions --- */
            /* --------------------------- */
            (SystemState::StartingUp(_), Transition::Idle) => Ok(SystemState::Healthy),
            (SystemState::StartingUp(_), _) => Err(Error::InvalidStateTransition(format!(
                "{} -> {}",
                self, &next
            ))),

            /* --------------------------- */
            /* --- Standby Transitions --- */
            /* --------------------------- */
            (_, _) => Err(Error::InvalidStateTransition(format!(
                "{} -> {}",
                self, &next
            ))),
        };

        match result {
            Ok(next_state) => {
                *self = next_state;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub fn set_error(&mut self, message: String) -> Result<(), Error> {
        self.transition(Transition::Error(message))
    }

    pub fn panic(&mut self, message: String) -> Result<(), Error> {
        self.transition(Transition::Panic(message))
    }
}
