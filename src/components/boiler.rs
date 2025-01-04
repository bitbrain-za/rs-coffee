use crate::config::{self, Boiler as Config};
use crate::gpio::pwm::PwmBuilder;
use crate::models::boiler::{BoilerModel, BoilerModelParameters};
use crate::types::Temperature;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::OutputPin;
use std::sync::{
    mpsc::{channel, Sender},
    Arc, RwLock,
};
use std::time::{Duration, Instant};

const UPDATE_INTERVAL: u64 = 1000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    Off,
    Transparent {
        power: f32,
    },
    BangBang {
        upper_threshold: f32,
        lower_threshold: f32,
    },
    Mpc {
        target: f32,
    },
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Transparent { power } => write!(f, "Transparent: {:.2}W", power),
            Mode::Off => write!(f, "Off"),
            Mode::BangBang {
                upper_threshold,
                lower_threshold,
            } => write!(f, "BangBang: {} - {}", upper_threshold, lower_threshold),
            Mode::Mpc { target } => write!(f, "Mpc: {}", target),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Message {
    SetMode(Mode),
    UpdateParameters {
        parameters: BoilerModelParameters,
        initial_probe_temperature: f32,
        initial_boiler_temperature: f32,
    },
}

impl Message {
    fn handle(&self, boiler: &mut BoilerModel, my_mode: &mut Mode) {
        match *self {
            Message::SetMode(mode) => {
                *my_mode = mode;
            }
            Message::UpdateParameters {
                parameters,
                initial_probe_temperature,
                initial_boiler_temperature,
            } => {
                boiler.update_parameters(
                    parameters,
                    initial_probe_temperature,
                    initial_boiler_temperature,
                );
            }
        }
    }
}

pub type Mailbox = Sender<Message>;

#[derive(Clone)]
pub struct Boiler {
    mailbox: Mailbox,
}

impl Boiler {
    pub fn send_message(&self, message: Message) {
        self.mailbox.send(message).unwrap();
    }

    pub fn new<PE>(
        ambient_probe: Arc<RwLock<Temperature>>,
        temperature_probe: Arc<RwLock<Temperature>>,
        element_pin: PE,
        config: Config,
    ) -> Self
    where
        PE: OutputPin,
    {
        let model = BoilerModel::new(ambient_probe.clone(), None, config);
        let (mailbox, rx) = channel::<Message>();
        let mut element = PwmBuilder::new()
            .with_interval(config.pwm_period)
            .with_pin(element_pin)
            .build();

        #[cfg(feature = "simulate")]
        let boiler_simulator = crate::models::boiler::BoilerModel::new(Some(25.0));
        let mut next_iteration = Instant::now() + Duration::from_millis(UPDATE_INTERVAL);

        std::thread::Builder::new()
            .name("Boiler".to_string())
            .spawn(move || {
                let mut my_mode = Mode::Off;
                let mut duty_cycle = 0.0;
                let mut my_boiler_model = model;
                #[cfg(feature = "simulate")]
                let mut boiler_simulator = boiler_simulator;
                #[cfg(feature = "simulate")]
                {
                    boiler_simulator.max_power = config::BOILER_POWER;
                }

                loop {
                    while let Ok(message) = rx.try_recv() {
                        message.handle(&mut my_boiler_model, &mut my_mode);
                    }

                    duty_cycle = match my_mode {
                        Mode::Off => 0.0,
                        Mode::Transparent { power } => power / config.power,
                        Mode::BangBang {
                            upper_threshold,
                            lower_threshold,
                        } => {
                            if next_iteration > Instant::now() {
                                continue;
                            }
                            next_iteration += Duration::from_secs_f32(
                                UPDATE_INTERVAL as f32 * config::TIME_DILATION_FACTOR / 1000.0,
                            );
                            let probe_temperature = *temperature_probe.read().unwrap();
                            if probe_temperature >= upper_threshold {
                                0.0
                            } else if probe_temperature <= lower_threshold {
                                1.0
                            } else {
                                duty_cycle
                            }
                        }
                        Mode::Mpc { target } => {
                            if next_iteration > Instant::now() {
                                continue;
                            }
                            let probe_temperature = *temperature_probe.read().unwrap();
                            let power = my_boiler_model.control(
                                probe_temperature,
                                *ambient_probe.read().unwrap(),
                                target,
                                Duration::from_millis(UPDATE_INTERVAL),
                            );

                            my_boiler_model.update(power, Duration::from_millis(UPDATE_INTERVAL));
                            next_iteration += Duration::from_secs_f32(
                                UPDATE_INTERVAL as f32 * config::TIME_DILATION_FACTOR / 1000.0,
                            );
                            my_boiler_model.get_duty_cycle()
                        }
                    };

                    #[cfg(feature = "simulate")]
                    {
                        let (_, probe) = boiler_simulator.update(
                            duty_cycle * boiler_simulator.max_power,
                            Duration::from_millis(UPDATE_INTERVAL),
                        );
                        *temperature_probe.write().unwrap() = probe;
                    }
                    {
                        element.set_duty_cycle(duty_cycle);
                        element.tick();
                    }
                    FreeRtos::delay_ms((config::TIME_DILATION_FACTOR * 1000.0) as u32);
                }
            })
            .expect("Failed to spawn output thread");

        Self { mailbox }
    }
}
