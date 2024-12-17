#[derive(Debug, Clone)]
pub enum SystemState {
    StartingUp(String),
    Idle,
    Standby(f32),
    Heating(f32),
    Ready,
    PreInfusing,
    Brewing,
    Steaming,
    HotWater,
    Cleaning,
    Error(String),
    Panic(String),
}

pub enum Transition {
    Idle,
    Standby(f32),
    Heat(f32),
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
            SystemState::Idle => write!(f, "Idle"),
            SystemState::Standby(temperature) => write!(f, "Idling at {:.2}°C", temperature),
            SystemState::Heating(temperature) => write!(f, "Heating: {:.2}°C", temperature),
            SystemState::Ready => write!(f, "Ready"),
            SystemState::PreInfusing => write!(f, "PreInfusing"),
            SystemState::Brewing => write!(f, "Brewing"),
            SystemState::Steaming => write!(f, "Steaming"),
            SystemState::HotWater => write!(f, "HotWater"),
            SystemState::Cleaning => write!(f, "Cleaning"),
            SystemState::Error(message) => write!(f, "Error: {}", message),
            SystemState::Panic(message) => write!(f, "Panic: {}", message),
        }
    }
}

impl std::fmt::Display for Transition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Transition::Idle => write!(f, "ReturnToIdle"),
            Transition::Standby(temperature) => write!(f, "Standby: {:.2}°C", temperature),
            Transition::Heat(temperature) => write!(f, "Heating to: {:.2}°C", temperature),
            Transition::Error(message) => write!(f, "Error: {}", message),
            Transition::ClearErrros => write!(f, "ClearErrors"),
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
            (SystemState::Error(_), Transition::ClearErrros) => Ok(SystemState::Idle),

            /* We are in an error state and new state comes our way (not panic, not error) */
            (SystemState::Error(_), _) => Err(Error::SystemInErrorState(self.to_string())),

            /* We are not in a error or panic state and error comes */
            (_, Transition::Error(message)) => Ok(SystemState::Error(message.clone())),

            /* --------------------------- */
            /* --- Startup Transitions --- */
            /* --------------------------- */
            (SystemState::StartingUp(_), Transition::Idle) => Ok(SystemState::Idle),
            (SystemState::StartingUp(_), Transition::Standby(temperature)) => {
                Ok(SystemState::Standby(*temperature))
            }
            (SystemState::StartingUp(_), _) => Err(Error::InvalidStateTransition(format!(
                "{} -> {}",
                self, &next
            ))),

            /* ------------------------ */
            /* --- Idle Transitions --- */
            /* ------------------------ */
            (SystemState::Idle, Transition::Standby(temperature)) => {
                Ok(SystemState::Standby(*temperature))
            }
            (SystemState::Idle, Transition::Heat(temperature)) => {
                Ok(SystemState::Heating(*temperature))
            }
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
}

#[derive(Debug)]
pub enum Error {
    InvalidStateTransition(String),
    InvalidState(String),
    SystemAlreadyInHigherErrorState(String),
    SystemInErrorState(String),
    SystemInPanicState(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InvalidStateTransition(message) => {
                write!(f, "InvalidStateTransition: {}", message)
            }
            Error::InvalidState(message) => write!(f, "InvalidState: {}", message),
            Error::SystemAlreadyInHigherErrorState(message) => {
                write!(f, "Can't override current error state: {}", message)
            }
            Error::SystemInPanicState(message) => write!(f, "SystemInPanicState: {}", message),
            Error::SystemInErrorState(message) => write!(f, "SystemInErrorState: {}", message),
        }
    }
}

impl std::error::Error for Error {
    fn description(&self) -> &str {
        match self {
            Error::InvalidStateTransition(_) => "Invalid state transition",
            Error::InvalidState(_) => "Invalid state",
            Error::SystemAlreadyInHigherErrorState(_) => "System already in higher error state",
            Error::SystemInErrorState(_) => "System in error state",
            Error::SystemInPanicState(_) => "System in panic state",
        }
    }
}
