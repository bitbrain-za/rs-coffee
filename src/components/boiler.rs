use crate::app_state::System;
use crate::board::Element;
use crate::board::F32Read::BoilerTemperature;
use crate::config;
use crate::models::boiler::{BoilerModel, BoilerModelParameters};
use esp_idf_svc::hal::delay::FreeRtos;
use std::time::{Duration, Instant};
use std::{
    sync::{Arc, Mutex},
    thread,
};

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
    Shutdown,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Shutdown => write!(f, "Shutdown"),
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
    Kill,
    SetMode(Mode),
    UpdateParameters {
        parameters: BoilerModelParameters,
        initial_probe_temperature: f32,
        initial_ambient_temperature: f32,
        initial_boiler_temperature: f32,
    },
}

impl Message {
    fn handle(&self, boiler: &mut BoilerModel, my_mode: &mut Mode) {
        match *self {
            Message::Kill => {
                log::info!("Boiler thread shutting down");
                *my_mode = Mode::Shutdown;
            }
            Message::SetMode(mode) => {
                // log::info!("Setting mode: {}", mode);
                *my_mode = mode;
            }
            Message::UpdateParameters {
                parameters,
                initial_probe_temperature,
                initial_ambient_temperature,
                initial_boiler_temperature,
            } => {
                log::info!("Updating parameters");
                boiler.update_parameters(
                    parameters,
                    initial_probe_temperature,
                    initial_boiler_temperature,
                    initial_ambient_temperature,
                );
            }
        }
    }
}

pub type Mailbox = Arc<Mutex<Vec<Message>>>;
pub struct Boiler {
    mailbox: Mailbox,
    pub system: System,
    handle: Option<thread::JoinHandle<()>>,
}

impl Boiler {
    pub fn new(system: System) -> Self {
        Self {
            system,
            handle: None,
            mailbox: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn get_mailbox(&self) -> Mailbox {
        self.mailbox.clone()
    }

    pub fn start(&mut self, element: Element) {
        let model = BoilerModel::new(Some(config::STAND_IN_AMBIENT));
        let my_mailbox = self.mailbox.clone();
        let system = self.system.clone();
        #[cfg(not(feature = "simulate"))]
        let mut element = element;
        #[cfg(feature = "simulate")]
        let _ = element;
        #[cfg(feature = "simulate")]
        let boiler_simulator = crate::models::boiler::BoilerModel::new(Some(25.0));

        let mut next_iteration = Instant::now() + Duration::from_millis(UPDATE_INTERVAL);

        let handle = std::thread::Builder::new()
            .name("Boiler".to_string())
            .spawn(move || {
                let mut my_mode = Mode::Off;
                let mut duty_cycle = 0.0;
                let mut my_boiler_model = model;
                #[cfg(feature = "simulate")]
                let mut boiler_simulator = boiler_simulator;
                #[cfg(feature = "simulate")]
                {
                    boiler_simulator.max_power = 1000.0;
                    boiler_simulator.print();
                }

                loop {
                    /* Check for messages */
                    let messages = my_mailbox
                        .lock()
                        .unwrap()
                        .drain(..)
                        .collect::<Vec<Message>>();

                    for message in messages {
                        message.handle(&mut my_boiler_model, &mut my_mode);
                    }

                    duty_cycle = match my_mode {
                        Mode::Shutdown => break,
                        Mode::Off => 0.0,
                        Mode::Transparent { power } => power / config::BOILER_POWER * 100.0,
                        Mode::BangBang {
                            upper_threshold,
                            lower_threshold,
                        } => {
                            if next_iteration > Instant::now() {
                                continue;
                            }
                            next_iteration += Duration::from_millis(UPDATE_INTERVAL);
                            let probe_temperature = system.read_f32(BoilerTemperature);
                            if probe_temperature >= upper_threshold {
                                0.0
                            } else if probe_temperature <= lower_threshold {
                                100.0
                            } else {
                                duty_cycle
                            }
                        }
                        Mode::Mpc { target } => {
                            if next_iteration > Instant::now() {
                                continue;
                            }
                            let probe_temperature = system.read_f32(BoilerTemperature);
                            let power = my_boiler_model.control(
                                probe_temperature,
                                config::STAND_IN_AMBIENT,
                                target,
                                Duration::from_millis(UPDATE_INTERVAL),
                            );

                            my_boiler_model.update(power, Duration::from_millis(UPDATE_INTERVAL));
                            next_iteration += Duration::from_millis(UPDATE_INTERVAL);
                            my_boiler_model.get_duty_cycle()
                        }
                    };

                    #[cfg(feature = "simulate")]
                    {
                        let (_, probe) = boiler_simulator.update(
                            duty_cycle * boiler_simulator.max_power / 100.0,
                            Duration::from_millis(UPDATE_INTERVAL),
                        );

                        {
                            system
                                .board
                                .clone()
                                .lock()
                                .unwrap()
                                .sensors
                                .temperature
                                .lock()
                                .unwrap()
                                .set_temperature(probe);
                        }

                        FreeRtos::delay_ms(10);
                    }
                    #[cfg(not(feature = "simulate"))]
                    {
                        element.set_duty_cycle(duty_cycle);
                        let mut next_update: Vec<Duration> =
                            vec![Duration::from_millis(UPDATE_INTERVAL)];
                        if let Some(duration) = element.tick() {
                            next_update.push(duration);
                        }
                        FreeRtos::delay_ms(next_update.iter().min().unwrap().as_millis() as u32);
                    }
                }
            })
            .expect("Failed to spawn output thread");

        self.handle = Some(handle);
    }
}
