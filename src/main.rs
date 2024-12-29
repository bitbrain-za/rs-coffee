// [ ] Go through all the "expects" and change them to put the system into an error/panic state
// [ ] Remove this later, just silence warnings while we're doing large scale writing
#![allow(dead_code)]
mod api;
mod app_state;
mod board;
mod components;
mod config;
mod gpio;
mod indicator;
mod kv_store;
mod models;
mod schemas;
mod sensors;
mod state_machines;
use crate::components::boiler::Message as BoilerMessage;
use anyhow::Result;
use app_state::System;
use board::{Action, F32Read, Reading};
use dotenv_codegen::dotenv;
use state_machines::operational_fsm::OperationalState;
use state_machines::system_fsm::{SystemState, Transition as SystemTransition};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const SIMULATE_AUTO_TUNE: bool = false;
#[cfg(feature = "simulate")]
fn simulate_auto_tuner(system: System, mailbox: crate::components::boiler::Mailbox) {
    if SIMULATE_AUTO_TUNE {
        log::info!("Running simulation");
        let mut auto_tuner =
            models::auto_tune::HeuristicAutoTuner::new(Duration::from_millis(1000), system.clone());
        auto_tuner.boiler_mailbox = Some(mailbox.clone());
        match auto_tuner.auto_tune_blocking() {
            Ok(res) => {
                let probe_temperature = system.read_f32(board::F32Read::BoilerTemperature);
                let message = components::boiler::Message::UpdateParameters {
                    parameters: res,
                    initial_probe_temperature: probe_temperature,
                    initial_ambient_temperature: config::STAND_IN_AMBIENT,
                    initial_boiler_temperature: auto_tuner.get_model_boiler_temperature(),
                };
                mailbox.lock().unwrap().push(message);
                let message = components::boiler::Message::SetMode(components::boiler::Mode::Mpc {
                    target: 94.0,
                });
                mailbox.lock().unwrap().push(message);
            }
            Err(e) => log::error!("{:?}", e),
        }
    }
}

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let logger = esp_idf_svc::log::EspLogger;
    logger
        .set_target_level("*", log::LevelFilter::Debug)
        .unwrap();
    logger
        .set_target_level("rmt(legacy)", log::LevelFilter::Info)
        .unwrap();
    logger
        .set_target_level("efuse", log::LevelFilter::Info)
        .unwrap();

    log::info!("Starting up");

    let (system, element) = System::new();
    {
        let board = system.board.lock().unwrap();
        *board.outputs.boiler_duty_cycle.lock().unwrap() = 0.5;
        *board.outputs.pump_duty_cycle.lock().unwrap() = 0.2;
        *board.outputs.solenoid.lock().unwrap() =
            gpio::relay::State::on(Some(Duration::from_secs(5)));
    }
    let api_state = app_state::ApiData {
        echo_data: "Init".to_string(),
        drink: None,
    };
    let api_state = Arc::new(Mutex::new(api_state));
    let server = api::rest::create_server(api_state.clone())?;
    core::mem::forget(server);

    let mqtt_url: String = dotenv!("MQTT_SERVER").try_into().expect("Invalid MQTT URL");
    let mqtt_port: u16 = dotenv!("MQTT_PORT").parse().expect("Invalid MQTT Port");
    let mqtt_url = format!("mqtt://{}:{}", mqtt_url, mqtt_port);
    let mqtt_client_id = dotenv!("MQTT_CLIENT_ID");
    api::mqtt::mqtt_create(&mqtt_url, mqtt_client_id);

    let mut boiler = components::boiler::Boiler::new(system.clone());
    boiler.start(element);

    simulate_auto_tuner(system.clone(), boiler.get_mailbox());

    let mut loop_interval = Duration::from_millis(1000);
    let mut auto_tuner =
        models::auto_tune::HeuristicAutoTuner::new(Duration::from_millis(1000), system.clone());

    system
        .system_state
        .lock()
        .unwrap()
        .transition(SystemTransition::Idle)
        .expect("Invalid transition :(");

    loop {
        let system_state = system.system_state.lock().unwrap().clone();
        let operational_state = system.operational_state.lock().unwrap().clone();

        match (system_state, operational_state) {
            (SystemState::Healthy, operational_state) => {
                let boiler_temperature = system.read_f32(F32Read::BoilerTemperature);
                let pump_pressure = system.read_f32(F32Read::PumpPressure);

                match operational_state {
                    OperationalState::Idle => {
                        log::debug!("Boiler temperature: {}", boiler_temperature);
                        log::debug!("Pump pressure: {}", pump_pressure);
                        log::debug!("Weight: {}", system.read_f32(F32Read::ScaleWeight));
                        let _ = system.execute_board_action(Action::SetIndicator(
                            indicator::ring::State::Idle,
                        ));
                    }
                    OperationalState::Brewing => {
                        let indicator = indicator::ring::State::Temperature {
                            min: 25.0,
                            max: 100.0,
                            level: boiler_temperature,
                        };
                        let _ = system.execute_board_action(Action::SetIndicator(indicator));
                    }
                    OperationalState::Steaming => {
                        let indicator = indicator::ring::State::Temperature {
                            min: 25.0,
                            max: 140.0,
                            level: boiler_temperature,
                        };
                        let _ = system.execute_board_action(Action::SetIndicator(indicator));
                    }
                    OperationalState::AutoTuneInit => {
                        log::info!("Auto-tuning boiler");

                        boiler
                            .get_mailbox()
                            .lock()
                            .unwrap()
                            .push(BoilerMessage::SetMode(components::boiler::Mode::Off));

                        system
                            .operational_state
                            .lock()
                            .unwrap()
                            .transition(
                                crate::state_machines::operational_fsm::Transitions::StartAutoTune,
                            )
                            .expect("Invalid transition :(");

                        #[cfg(feature = "simulate")]
                        {
                            loop_interval = Duration::from_millis(10);
                        }
                        auto_tuner = models::auto_tune::HeuristicAutoTuner::new(
                            Duration::from_millis(1000),
                            system.clone(),
                        );
                    }
                    OperationalState::AutoTuning => {
                        #[cfg(feature = "simulate")]
                        {
                            if let Some(res) = auto_tuner.run()? {
                                log::info!("Simulation completed");
                                log::info!("Results: {:?}", res);

                                let initial_boiler = auto_tuner.get_model_boiler_temperature();

                                let message = BoilerMessage::UpdateParameters {
                                    parameters: res,
                                    initial_probe_temperature: boiler_temperature,
                                    initial_ambient_temperature: config::STAND_IN_AMBIENT,
                                    initial_boiler_temperature: initial_boiler,
                                };

                                boiler.get_mailbox().lock().unwrap().push(message);

                                system
                                    .operational_state
                                    .lock()
                                    .unwrap()
                                    .transition(crate::state_machines::operational_fsm::Transitions::AutoTuneComplete)
                                    .expect("Invalid transition :(");
                            }
                        }
                    }
                    _ => {}
                }
            }
            (SystemState::Error(message), _) => {
                log::error!("System is in an error state: {}", message);
            }
            (SystemState::Panic(message), _) => {
                log::error!("System is in a panic state: {}", message);
            }

            (_, _) => {
                log::error!("unhandled state")
            }
        }

        if let Reading::AllButtonsState(Some(presses)) =
            system.do_board_read(Reading::AllButtonsState(None))
        {
            if !presses.is_empty() {
                for button in presses {
                    log::info!("Button pressed: {}", button);

                    if button == board::ButtonEnum::Brew {
                        let _ = system
                            .execute_board_action(Action::OpenValve(Some(Duration::from_secs(5))));

                        let mode = components::boiler::Mode::Mpc { target: 94.0 };
                        boiler
                            .get_mailbox()
                            .lock()
                            .unwrap()
                            .push(BoilerMessage::SetMode(mode));
                    }
                    if button == board::ButtonEnum::HotWater {
                        let mode = components::boiler::Mode::BangBang {
                            upper_threshold: 95.0,
                            lower_threshold: 85.0,
                        };
                        boiler
                            .get_mailbox()
                            .lock()
                            .unwrap()
                            .push(BoilerMessage::SetMode(mode));
                    }
                    if button == board::ButtonEnum::Steam {
                        boiler
                            .get_mailbox()
                            .lock()
                            .unwrap()
                            .push(BoilerMessage::SetMode(components::boiler::Mode::Off));
                    }
                }
            }
        }
        thread::sleep(loop_interval);
    }
}
