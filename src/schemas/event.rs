use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum LevelFilter {
    Panic,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl std::fmt::Display for LevelFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LevelFilter::Panic => write!(f, "Panic"),
            LevelFilter::Error => write!(f, "Error"),
            LevelFilter::Warn => write!(f, "Warn"),
            LevelFilter::Info => write!(f, "Info"),
            LevelFilter::Debug => write!(f, "Debug"),
            LevelFilter::Trace => write!(f, "Trace"),
        }
    }
}

impl From<&LevelFilter> for u8 {
    fn from(level: &LevelFilter) -> u8 {
        match level {
            LevelFilter::Panic => 0,
            LevelFilter::Error => 1,
            LevelFilter::Warn => 2,
            LevelFilter::Info => 3,
            LevelFilter::Debug => 4,
            LevelFilter::Trace => 5,
        }
    }
}

impl PartialOrd for LevelFilter {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(u8::from(self).cmp(&u8::from(other)))
    }
}

pub struct EventBuffer {
    pub events: Vec<Event>,
}

impl EventBuffer {
    pub fn new() -> Self {
        EventBuffer { events: Vec::new() }
    }

    pub fn push(&mut self, event: Event) {
        self.events.push(event);
    }

    pub fn panic(&mut self, source: &str, message: String) {
        self.push(Event::panic(source, message));
    }

    pub fn error(&mut self, source: &str, message: String) {
        self.push(Event::error(source, message));
    }

    pub fn warn(&mut self, source: &str, message: String) {
        self.push(Event::warn(source, message));
    }

    pub fn info(&mut self, source: &str, message: String) {
        self.push(Event::info(source, message));
    }

    pub fn debug(&mut self, source: &str, message: String) {
        self.push(Event::debug(source, message));
    }

    pub fn trace(&mut self, source: &str, message: String) {
        self.push(Event::trace(source, message));
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Event {
    pub level: LevelFilter,
    pub source: String,
    pub message: String,
}

impl Event {
    pub fn panic(source: &str, message: String) -> Event {
        Event {
            level: LevelFilter::Panic,
            source: source.to_string(),
            message,
        }
    }

    pub fn error(source: &str, message: String) -> Event {
        Event {
            level: LevelFilter::Error,
            source: source.to_string(),
            message,
        }
    }

    pub fn warn(source: &str, message: String) -> Event {
        Event {
            level: LevelFilter::Warn,
            source: source.to_string(),
            message,
        }
    }

    pub fn info(source: &str, message: String) -> Event {
        Event {
            level: LevelFilter::Info,
            source: source.to_string(),
            message,
        }
    }

    pub fn debug(source: &str, message: String) -> Event {
        Event {
            level: LevelFilter::Debug,
            source: source.to_string(),
            message,
        }
    }

    pub fn trace(source: &str, message: String) -> Event {
        Event {
            level: LevelFilter::Trace,
            source: source.to_string(),
            message,
        }
    }
}

impl Event {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}
