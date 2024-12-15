use esp_idf_svc::hal::gpio::{Output, OutputPin, PinDriver};
use std::time::{Duration, Instant};

#[derive(Copy, Clone, Debug, std::default::Default, PartialEq)]
pub enum State {
    On,
    #[default]
    Off,
    OnUntil(Instant),
    OffUntil(Instant),
}

impl Iterator for State {
    type Item = State;

    fn next(&mut self) -> Option<Self::Item> {
        let next = match self {
            State::On => None,
            State::Off => None,
            State::OnUntil(off_instant) => {
                if Instant::now() < *off_instant {
                    return Some(State::OnUntil(*off_instant));
                }
                Some(State::Off)
            }
            State::OffUntil(on_instant) => {
                if Instant::now() < *on_instant {
                    return Some(State::OffUntil(*on_instant));
                }
                Some(State::On)
            }
        };
        if let Some(next) = next {
            *self = next;
        }
        next
    }
}

impl State {
    pub fn on(on_time: Option<Duration>) -> Self {
        if let Some(on_time) = on_time {
            State::OnUntil(Instant::now() + on_time)
        } else {
            State::On
        }
    }

    pub fn off(off_time: Option<Duration>) -> Self {
        if let Some(off_time) = off_time {
            State::OffUntil(Instant::now() + off_time)
        } else {
            State::Off
        }
    }
}

pub struct Relay<'a, PD: OutputPin> {
    out: PinDriver<'a, PD, Output>,
    invert: bool,
    pub state: State,
}

impl<'a, PD> Relay<'a, PD>
where
    PD: OutputPin,
{
    pub fn new(pin: PD, invert: Option<bool>) -> Self {
        Relay {
            out: PinDriver::output(pin).expect("Failed to create relay"),
            invert: invert.unwrap_or(false),
            state: State::Off,
        }
    }

    #[allow(dead_code)]
    pub fn turn_on(&mut self, on_time: Option<Duration>) {
        self.set_state(State::on(on_time));
    }

    #[allow(dead_code)]
    pub fn turn_off(&mut self, off_time: Option<Duration>) {
        self.set_state(State::off(off_time));
    }

    fn set_state(&mut self, state: State) -> Option<Duration> {
        self.state = state;
        match self.state {
            State::On => {
                self.set_on();
                None
            }
            State::OnUntil(instant) => {
                self.set_on();
                Some(instant - Instant::now())
            }
            State::Off => {
                self.set_off();
                None
            }
            State::OffUntil(instant) => {
                self.set_off();
                Some(instant - Instant::now())
            }
        }
    }

    fn set_on(&mut self) {
        match self.invert {
            true => self.out.set_low(),
            false => self.out.set_high(),
        }
        .expect("Failed to set relay on");
    }

    fn set_off(&mut self) {
        match self.invert {
            true => self.out.set_high(),
            false => self.out.set_low(),
        }
        .expect("Failed to set relay off");
    }

    pub fn tick(&mut self) -> Option<Duration> {
        let next_state = self.state.next();
        if let Some(next_state) = next_state {
            self.set_state(next_state)
        } else {
            None
        }
    }
}
