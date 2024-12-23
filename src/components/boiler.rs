use crate::app_state::System;
use crate::board::Element;
use crate::board::F32Read::BoilerTemperature;
#[cfg(not(feature = "simulate"))]
use crate::components::boiler;
#[cfg(not(feature = "simulate"))]
use esp_idf_svc::hal::delay::FreeRtos;
use std::time::Duration;
use std::{
    sync::{Arc, Mutex},
    thread,
};

const UPDATE_INTERVAL: u64 = 1000;

#[derive(Clone, Copy)]
pub enum Mode {
    Off,
    BangBang {
        upper_threshold: f32,
        lower_threshold: f32,
    },
    Mpc {
        target: f32,
    },
    AutoTune {
        target: f32,
    },
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Off => write!(f, "Off"),
            Mode::BangBang {
                upper_threshold,
                lower_threshold,
            } => write!(f, "BangBang: {} - {}", upper_threshold, lower_threshold),
            Mode::Mpc { target } => write!(f, "Mpc: {}", target),
            Mode::AutoTune { target } => write!(f, "AutoTune: {}", target),
        }
    }
}

pub struct Boiler {
    pub mode: Arc<Mutex<Mode>>,
    pub system: System,
    handle: Option<thread::JoinHandle<()>>,
    kill_switch: Arc<Mutex<bool>>,
}

impl Boiler {
    pub fn new(system: System) -> Self {
        Self {
            system,
            mode: Arc::new(Mutex::new(Mode::Off)),
            kill_switch: Arc::new(Mutex::new(false)),
            handle: None,
        }
    }

    pub fn set(&mut self, mode: Mode) {
        log::debug!("Setting: {}", mode);
        *self.mode.lock().unwrap() = mode
    }

    pub fn start(&mut self, element: Element) {
        let kill_switch_clone = self.kill_switch.clone();
        let my_mode = self.mode.clone();
        let system = self.system.clone();
        #[cfg(not(feature = "simulate"))]
        let mut element = element;
        #[cfg(feature = "simulate")]
        let boiler_simulator = crate::models::boiler::BoilerModel::new(Some(25.0));
        #[cfg(feature = "simulate")]
        let _ = element;

        let handle = std::thread::Builder::new()
            .name("Boiler".to_string())
            .spawn(move || {
                let mut duty_cycle = 0.0;
                #[cfg(feature = "simulate")]
                let mut boiler_simulator = boiler_simulator;
                #[cfg(feature = "simulate")]
                {
                    boiler_simulator.max_power = 1000.0;
                    boiler_simulator.print();
                }

                loop {
                    if *kill_switch_clone.lock().unwrap() {
                        log::info!("Boiler thread killed");
                        *my_mode.lock().unwrap() = Mode::Off;
                        return;
                    }

                    let mode = *my_mode.lock().unwrap();

                    duty_cycle = match mode {
                        Mode::Off => 0.0,
                        Mode::BangBang {
                            upper_threshold,
                            lower_threshold,
                        } => {
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
                            let _ = target;
                            todo!();
                        }
                        Mode::AutoTune { target } => {
                            let _ = target;
                            todo!();
                        }
                    };

                    #[cfg(feature = "simulate")]
                    {
                        let (_, probe) = boiler_simulator.update(
                            duty_cycle * boiler_simulator.max_power / 100.0,
                            Duration::from_millis(1000),
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

                        esp_idf_svc::hal::delay::FreeRtos::delay_ms(10);
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
