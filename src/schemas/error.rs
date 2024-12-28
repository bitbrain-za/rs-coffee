#[derive(Debug)]
pub enum Error {
    MissingOutputSpecifier,
    MissingProfile,
    InvalidProfile(String),
    OutOfBounds(String),
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::MissingOutputSpecifier => write!(f, "Shot must specify either weight or time"),
            Error::MissingProfile => write!(f, "Shot must have at least one profile"),
            Error::InvalidProfile(reason) => write!(f, "Invalid profile: {}", reason),
            Error::OutOfBounds(reason) => write!(f, "Value out of bounds: {}", reason),
        }
    }
}
