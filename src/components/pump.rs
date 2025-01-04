use crate::config::Pump as Config;
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
    Backflush,
}

pub type Mailbox = Sender<Message>;

#[derive(Clone)]
pub struct Pump {
    mailbox: Mailbox,
}

impl Pump {
    pub fn new<PD: OutputPin, PE: OutputPin>(
        pump_pin: PD,
        solenoid_pin: PE,
        pressure_probe: Arc<RwLock<Bar>>,
        weight_probe: Arc<RwLock<Grams>>,
        config: Config,
    ) -> Self {
        PumpInternal::start(pump_pin, solenoid_pin, pressure_probe, weight_probe, config)
    }
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
    pub fn turn_on_for_hot_water(&self) {
        self.mailbox.send(Message::OnForHotWater).unwrap();
    }
    pub fn backflush(&self) {
        self.mailbox.send(Message::Backflush).unwrap();
    }
}

enum State {
    On(Option<Instant>),
    Off,
    OnForYield { start: Grams, target: Grams },
    Backflush,
}

struct PumpInternal<PD: OutputPin, PE: OutputPin> {
    pwm: Pwm<'static, PD>,
    solenoid: PinDriver<'static, PE, Output>,
    pressure_probe: Arc<RwLock<Bar>>,
    weight_probe: Arc<RwLock<Grams>>,
    state: State,
    backflush_cycle_start: Instant,
    backflush_in_off_cycle: bool,
    config: Config,
}

impl<PD, PE> PumpInternal<PD, PE>
where
    PD: OutputPin,
    PE: OutputPin,
{
    fn start(
        pump_pin: PD,
        solenoid_pin: PE,
        pressure_probe: Arc<RwLock<Bar>>,
        weight_probe: Arc<RwLock<Grams>>,
        config: Config,
    ) -> Pump {
        let (tx, rx) = channel();

        std::thread::spawn(move || {
            let mut my_pump = PumpInternal {
                pwm: Pwm::new(pump_pin, config.pwm_period, None),
                solenoid: PinDriver::output(solenoid_pin).expect("Failed to create relay"),
                pressure_probe,
                weight_probe,
                state: State::Off,
                backflush_cycle_start: Instant::now(),
                backflush_in_off_cycle: true,
                config,
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
                    State::Backflush => {
                        let elapsed = my_pump.backflush_cycle_start.elapsed();

                        if elapsed > config.backflush_off_time + config.backflush_on_time {
                            my_pump.backflush_cycle_start = Instant::now();
                            my_pump.open_valve();
                            my_pump.set_pressure(config.max_pressure);
                            my_pump.backflush_in_off_cycle = false;
                        } else if elapsed > config.backflush_on_time
                            && !my_pump.backflush_in_off_cycle
                        {
                            my_pump.backflush_in_off_cycle = true;
                            my_pump.close_valve();
                            my_pump.set_pressure(0.0);
                        }
                    }
                    _ => {}
                }

                let next_tick = [Some(config.pwm_period), my_pump.pwm.tick()]
                    .iter()
                    .filter_map(|x| *x)
                    .min()
                    .unwrap(); // this is safe, we've already inserted a default value

                std::thread::sleep(next_tick);
            }
        });
        Pump { mailbox: tx }
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
                self.set_pressure(self.config.max_pressure);
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
                self.set_pressure(self.config.max_pressure);
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
                self.set_pressure(self.config.max_pressure);
            }
            Message::Backflush => {
                self.state = State::Backflush;
                self.backflush_cycle_start = Instant::now();
                self.backflush_in_off_cycle = false;
                self.open_valve();
                self.set_pressure(self.config.max_pressure);
            }
        }
    }

    // [ ] this needs to be calibrated, for now it's a guess
    fn duty_cycle_to_pressure(&self, duty_cycle: f32) -> Bar {
        duty_cycle.clamp(0.0, 1.0) * self.config.max_pressure
    }

    // [ ] this needs to be calibrated, for now it's a guess
    fn pressure_to_duty_cycle(&self, pressure: f32) -> f32 {
        pressure.clamp(0.0, self.config.max_pressure) / self.config.max_pressure
    }
}
