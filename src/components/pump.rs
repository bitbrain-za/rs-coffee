use crate::config;
use crate::gpio::pwm::Pwm;
use crate::types::*;
use esp_idf_svc::hal::gpio::{Output, OutputPin, PinDriver};
use std::sync::{
    mpsc::{channel, Sender},
    Arc, RwLock,
};
use std::time::{Duration, Instant};

pub enum Message {
    On,
    Off,
    SetPressure(Bar),
    OnForTime(Duration),
    OnForTimeAtPressure(Duration, Bar),
    OnForYield { pressure: Bar, grams: Grams },
    OnForHotWater,
    Backflush(Duration),
}

pub type Mailbox = Sender<Message>;

#[derive(Clone)]
pub struct Interface {
    mailbox: Mailbox,
}

impl Interface {
    pub fn turn_on(&self, duration: Option<Duration>) {
        if let Some(duration) = duration {
            self.mailbox.send(Message::OnForTime(duration)).unwrap();
        } else {
            self.mailbox.send(Message::On).unwrap();
        }
    }
    pub fn turn_off(&self) {
        self.mailbox.send(Message::Off).unwrap();
    }
    pub fn set_pressure(&self, pressure: Bar) {
        self.mailbox.send(Message::SetPressure(pressure)).unwrap();
    }
    pub fn turn_on_for_yield(&self, pressure: Bar, grams: Grams) {
        self.mailbox
            .send(Message::OnForYield { pressure, grams })
            .unwrap();
    }
}

enum State {
    On(Option<Instant>),
    Off,
    OnForYield { start: Grams, target: Grams },
}

pub struct Pump<PD: OutputPin, PE: OutputPin> {
    pwm: Pwm<'static, PD>,
    solenoid: PinDriver<'static, PE, Output>,
    pressure_probe: Arc<RwLock<Bar>>,
    weight_probe: Arc<RwLock<Grams>>,
    state: State,
}

impl<PD, PE> Pump<PD, PE>
where
    PD: OutputPin,
    PE: OutputPin,
{
    pub fn start(
        pump_pin: PD,
        solenoid_pin: PE,
        pressure_probe: Arc<RwLock<Bar>>,
        weight_probe: Arc<RwLock<Grams>>,
        interval: Duration,
    ) -> Interface {
        let (tx, rx) = channel();

        std::thread::spawn(move || {
            let mut my_pump = Pump {
                pwm: Pwm::new(pump_pin, Duration::from_millis(100), None),
                solenoid: PinDriver::output(solenoid_pin).expect("Failed to create relay"),
                pressure_probe,
                weight_probe,
                state: State::Off,
            };
            loop {
                while let Ok(message) = rx.try_recv() {
                    my_pump.trasition(message);
                }

                match my_pump.state {
                    State::On(Some(end)) if Instant::now() > end => {
                        my_pump.trasition(Message::Off);
                    }
                    State::OnForYield { start, target } => {
                        let current_scale = *my_pump.weight_probe.read().unwrap();
                        if current_scale - start >= target {
                            my_pump.trasition(Message::Off);
                        }
                    }
                    _ => {}
                }

                let next_tick = [Some(interval), my_pump.pwm.tick()]
                    .iter()
                    .filter_map(|x| *x)
                    .min()
                    .unwrap(); // this is safe, we've already inserted a default value

                std::thread::sleep(next_tick);
            }
        });
        Interface { mailbox: tx }
    }

    fn set_pressure(&mut self, pressure: Bar) {
        self.pwm
            .set_duty_cycle(self.pressure_to_duty_cycle(pressure));
    }

    fn open_valve(&mut self) {
        self.solenoid.set_high().unwrap();
    }

    fn close_valve(&mut self) {
        self.solenoid.set_low().unwrap();
    }

    fn trasition(&mut self, message: Message) {
        match message {
            Message::On => {
                self.state = State::On(None);
                self.open_valve();
                self.set_pressure(config::MAX_PUMP_PRESSURE);
            }
            Message::Off => {
                self.state = State::Off;
                self.close_valve();
                self.pwm.set_duty_cycle(0.0);
            }
            Message::SetPressure(pressure) => {
                self.state = State::On(None);
                self.pwm
                    .set_duty_cycle(self.pressure_to_duty_cycle(pressure));
            }
            Message::OnForTime(duration) => {
                self.state = State::On(Some(Instant::now() + duration));
                self.open_valve();
                self.set_pressure(config::MAX_PUMP_PRESSURE);
            }
            Message::OnForTimeAtPressure(duration, pressure) => {
                self.state = State::On(Some(Instant::now() + duration));
                self.open_valve();
                self.set_pressure(pressure);
            }
            Message::OnForYield { pressure, grams } => {
                let current_scale = *self.weight_probe.read().unwrap();
                self.open_valve();
                self.state = State::OnForYield {
                    start: current_scale,
                    target: grams,
                };
                self.pwm
                    .set_duty_cycle(self.pressure_to_duty_cycle(pressure));
            }
            Message::OnForHotWater => {
                self.state = State::On(None);
                self.close_valve();
                self.set_pressure(config::MAX_PUMP_PRESSURE);
            }
            Message::Backflush(_) => {
                todo!();
            }
        }
    }

    // [ ] this needs to be calibrated, for now it's a guess
    fn duty_cycle_to_pressure(&self, duty_cycle: f32) -> Bar {
        duty_cycle.clamp(0.0, 1.0) * config::MAX_PUMP_PRESSURE
    }

    // [ ] this needs to be calibrated, for now it's a guess
    fn pressure_to_duty_cycle(&self, pressure: f32) -> f32 {
        pressure.clamp(0.0, config::MAX_PUMP_PRESSURE) / config::MAX_PUMP_PRESSURE
    }
}
