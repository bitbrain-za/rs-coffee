use esp_idf_svc::hal::gpio::{Output, OutputPin, PinDriver};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
enum State {
    On,
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
        if let Some(next) = next.clone() {
            *self = next;
        }
        next
    }
}

pub struct Relay<'a, PD: OutputPin> {
    out: PinDriver<'a, PD, Output>,
    invert: bool,
    current_state: State,
    poll_interval: Duration,
    last_poll: Instant,
}

impl<'a, PD> Relay<'a, PD>
where
    PD: OutputPin,
{
    pub fn new(pin: PD, invert: Option<bool>, poll_interval: Duration) -> Self {
        Relay {
            out: PinDriver::output(pin).expect("Failed to create relay"),
            invert: invert.unwrap_or(false),
            current_state: State::Off,
            poll_interval,
            last_poll: Instant::now() - poll_interval,
        }
    }

    pub fn turn_on(&mut self, on_time: Option<Duration>) {
        if let Some(on_time) = on_time {
            self.set_state(State::OnUntil(Instant::now() + on_time));
        } else {
            self.set_state(State::On);
        }
    }

    pub fn turn_off(&mut self, off_time: Option<Duration>) {
        if let Some(off_time) = off_time {
            self.set_state(State::OnUntil(Instant::now() + off_time));
        } else {
            self.set_state(State::Off);
        };
    }

    fn set_state(&mut self, state: State) {
        self.current_state = state;
        match self.current_state {
            State::On | State::OnUntil(_) => self.set_on(),
            _ => self.set_off(),
        };
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

    pub fn tick(&mut self) -> Duration {
        let time_since_last_poll = self.last_poll.elapsed();
        if time_since_last_poll < self.poll_interval {
            return self.poll_interval - time_since_last_poll;
        }

        let next_state = self.current_state.next();
        if let Some(next_state) = next_state {
            self.set_state(next_state);
        }

        self.last_poll = Instant::now();
        self.poll_interval
    }
}