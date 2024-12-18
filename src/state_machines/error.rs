#[derive(Debug)]
pub enum Error {
    InvalidStateTransition(String),
    InvalidState(String),
    SystemAlreadyInHigherErrorState(String),
    SystemInErrorState(String),
    SystemInPanicState(String),
    _Internal(String),
    NotYetImplemented,
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
            Error::_Internal(message) => write!(f, "InternalError: {}", message),
            Error::NotYetImplemented => write!(f, "NotYetImplemented"),
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
            Error::_Internal(_) => "Internal error",
            Error::NotYetImplemented => "Not yet implemented",
        }
    }
}
