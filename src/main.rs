// [ ] Go through all the "expects" and change them to put the system into an error/panic state
// [ ] Remove this later, just silence warnings while we're doing large scale writing
// #![allow(dead_code)]
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
mod types;
use crate::components::boiler::Message as BoilerMessage;
use anyhow::Result;
use app_state::System;
use dotenv_codegen::dotenv;
use gpio::switch::SwitchesState;
use sensors::ambient;
use state_machines::operational_fsm::OperationalState;
use state_machines::system_fsm::{SystemState, Transition as SystemTransition};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;

const SIMULATE_AUTO_TUNE: bool = false;
#[cfg(feature = "simulate")]
fn simulate_auto_tuner(
    temperature_probe: Arc<RwLock<f32>>,
    boiler: crate::components::boiler::Boiler,
) {
    if SIMULATE_AUTO_TUNE {
        log::info!("Running simulation");
        let mut auto_tuner = models::auto_tune::HeuristicAutoTuner::new(
            Duration::from_millis(1000),
            temperature_probe.clone(),
        );
        auto_tuner.boiler = Some(boiler.clone());
        match auto_tuner.auto_tune_blocking() {
            Ok(res) => {
                let probe_temperature = *temperature_probe.read().unwrap();
                let message = components::boiler::Message::UpdateParameters {
                    parameters: res,
                    initial_probe_temperature: probe_temperature,
                    initial_ambient_temperature: config::STAND_IN_AMBIENT,
                    initial_boiler_temperature: auto_tuner.get_model_boiler_temperature(),
                };
                boiler.send_message(message);
                let message = components::boiler::Message::SetMode(components::boiler::Mode::Mpc {
                    target: 94.0,
                });
                boiler.send_message(message);
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
        .set_target_level("*", log::LevelFilter::Info)
        .unwrap();
    logger
        .set_target_level("rmt(legacy)", log::LevelFilter::Info)
        .unwrap();
    logger
        .set_target_level("efuse", log::LevelFilter::Info)
        .unwrap();
    logger
        .set_target_level("temperature_sensor", log::LevelFilter::Info)
        .unwrap();
    logger
        .set_target_level("rs_coffee", log::LevelFilter::Debug)
        .unwrap();

    log::info!("Starting up");

    let system = System::new();
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
    api::mqtt::mqtt_create(
        &mqtt_url,
        mqtt_client_id,
        &system,
        Some(config::EVENT_LEVEL),
    );

    let temperature_probe = system.board.temperature.clone();
    let ambient_probe = system.board.ambient_temperature.clone();
    let boiler = system.board.boiler.clone();

    #[cfg(feature = "simulate")]
    simulate_auto_tuner(temperature_probe.clone(), boiler.clone());

    let mut loop_interval = Duration::from_millis(1000);
    let mut auto_tuner = models::auto_tune::HeuristicAutoTuner::new(
        Duration::from_millis(1000),
        temperature_probe.clone(),
    );

    info!(system, "Starting up");

    system
        .system_state
        .lock()
        .unwrap()
        .transition(SystemTransition::Idle)
        .expect("Invalid transition :(");

    let scale = system.board.scale.clone();
    let switches = system.board.switches.clone();
    let pressure_probe = system.board.pressure.clone();
    let pump = system.board.pump.clone();
    let board = system.board.clone();

    let mut previous_switch_state = SwitchesState::Idle;

    loop {
        let system_state = system.system_state.lock().unwrap().clone();
        let operational_state = system.operational_state.lock().unwrap().clone();

        match (system_state, operational_state) {
            (SystemState::Healthy, operational_state) => {
                let boiler_temperature = *temperature_probe.read().unwrap();
                let pump_pressure = *pressure_probe.read().unwrap();
                let ambient_temperature = *ambient_probe.read().unwrap();

                match operational_state {
                    OperationalState::Idle => {
                        log::debug!("Boiler temperature: {}", boiler_temperature);
                        log::debug!("Pump pressure: {}", pump_pressure);
                        log::debug!("Weight: {}", scale.get_weight());
                        log::debug!("Flow: {}", scale.get_flow());
                        log::debug!("Ambient temperature: {}", ambient_temperature);
                        board.indicator.set_state(indicator::ring::State::Idle);
                    }
                    OperationalState::Brewing => {
                        let indicator = indicator::ring::State::Temperature {
                            min: 25.0,
                            max: 100.0,
                            level: boiler_temperature,
                        };
                        board.indicator.set_state(indicator);
                    }
                    OperationalState::Steaming => {
                        let indicator = indicator::ring::State::Temperature {
                            min: 25.0,
                            max: 140.0,
                            level: boiler_temperature,
                        };
                        board.indicator.set_state(indicator);
                    }
                    OperationalState::AutoTuneInit => {
                        log::info!("Auto-tuning boiler");
                        info!(system, "Auto-tuning boiler");

                        boiler.send_message(BoilerMessage::SetMode(components::boiler::Mode::Off));

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
                            temperature_probe.clone(),
                        );
                    }
                    OperationalState::AutoTuning => {
                        #[cfg(feature = "simulate")]
                        {
                            if let Some(res) = auto_tuner.run()? {
                                log::info!("Simulation completed");
                                log::info!("Results: {:?}", res);
                                info!(system, "Simulation Results: {:?}", res);

                                let initial_boiler = auto_tuner.get_model_boiler_temperature();

                                let message = BoilerMessage::UpdateParameters {
                                    parameters: res,
                                    initial_probe_temperature: boiler_temperature,
                                    initial_ambient_temperature: config::STAND_IN_AMBIENT,
                                    initial_boiler_temperature: initial_boiler,
                                };

                                boiler.send_message(message);

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
                error!(system, "System is in an error state: {}", message);
            }
            (SystemState::Panic(message), _) => {
                log::error!("System is in a panic state: {}", message);
                panic!(system, "System is in a panic state: {}", message);
            }

            (_, _) => {
                log::error!("unhandled state")
            }
        }

        let current_state = switches.get_state();
        if previous_switch_state != current_state {
            if previous_switch_state == SwitchesState::Brew {
                system.board.scale.stop_brewing();
            }
            match current_state {
                SwitchesState::Idle => {
                    log::info!("Switched to idle");
                    info!(system, "Switched to idle");
                    boiler.send_message(BoilerMessage::SetMode(components::boiler::Mode::Off));
                    pump.turn_off();
                }
                SwitchesState::Brew => {
                    info!(system, "Switched to brew");
                    log::info!("Switched to brew");
                    system.board.scale.start_brew();
                    pump.turn_on(Some(Duration::from_secs(5)));
                    let mode = components::boiler::Mode::Mpc { target: 94.0 };
                    boiler.send_message(BoilerMessage::SetMode(mode));
                }
                SwitchesState::HotWater => {
                    log::info!("Switched to hot water");
                    let mode = components::boiler::Mode::Mpc { target: 94.0 };
                    boiler.send_message(BoilerMessage::SetMode(mode));
                    pump.turn_on_for_hot_water();
                }
                SwitchesState::Steam => {
                    log::info!("Switched to steam");
                    info!(system, "Switched to steam");
                    let mode = components::boiler::Mode::BangBang {
                        upper_threshold: 140.0,
                        lower_threshold: 120.0,
                    };
                    pump.turn_off();
                    boiler.send_message(BoilerMessage::SetMode(mode));
                }
                SwitchesState::Backflush => {
                    log::info!("Switched to backflush");
                    info!(system, "Switched to backflush");
                    let mode = components::boiler::Mode::Mpc { target: 70.0 };
                    boiler.send_message(BoilerMessage::SetMode(mode));
                    pump.backflush();
                }
                SwitchesState::AutoTune => {
                    log::info!("Switched to auto-tune");
                    info!(system, "Switched to auto-tune");
                    pump.turn_off();
                    if let Err(e) = system.operational_state.lock().unwrap().transition(
                        crate::state_machines::operational_fsm::Transitions::StartAutoTune,
                    ) {
                        log::warn!("Failed to transition to auto-tune: {:?}", e);
                        warn!(system, "Failed to transition to auto-tune: {:?}", e);
                    }
                }
            }
            previous_switch_state = current_state;
        }
        thread::sleep(loop_interval);
    }
}
