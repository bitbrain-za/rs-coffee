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
use gpio::switch::SwitchesState;
use state_machines::operational_fsm::OperationalState;
use state_machines::system_fsm::{SystemState, Transition as SystemTransition};
use std::thread;
use std::time::Duration;

#[cfg(feature = "simulate")]
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

    log::set_max_level(log::LevelFilter::Debug);

    esp_idf_svc::log::set_target_level("*", log::LevelFilter::Info).unwrap();
    esp_idf_svc::log::set_target_level("*", log::LevelFilter::Info).unwrap();
    esp_idf_svc::log::set_target_level("*", log::LevelFilter::Info).unwrap();
    esp_idf_svc::log::set_target_level("rs_coffee", log::LevelFilter::Debug).unwrap();

    log::info!("Starting up");

    let system = System::new();

    #[cfg(feature = "sdcard")]
    if *system.sd_card_present {
        log::info!("SD card is present");
    } else {
        log::warn!("SD card is not present, data will not be saved");
    }

    let server = api::rest::create_server(system.clone())?;
    core::mem::forget(server);

    let config_mqtt = system.config.read().unwrap().mqtt.clone();
    api::mqtt::mqtt_create(config_mqtt, &system);

    let temperature_probe = system.board.temperature.clone();
    let ambient_probe = system.board.ambient_temperature.clone();
    let boiler = system.board.boiler.clone();

    #[cfg(feature = "simulate")]
    simulate_auto_tuner(temperature_probe.clone(), boiler.clone());

    let mut loop_interval = Duration::from_millis(1000);
    let mut auto_tuner = models::auto_tune::HeuristicAutoTuner::new(
        Duration::from_millis(1000),
        temperature_probe.clone(),
        ambient_probe.clone(),
        system.config.read().unwrap().boiler.mpc.auto_tune,
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
    let level = system.board.level_sensor.clone();

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
                        log::debug!("Ambient temperature: {:.4}", ambient_temperature);
                        log::debug!("Boiler temperature: {:.4}", boiler_temperature);
                        log::debug!("Pump pressure: {:.2}", pump_pressure);
                        log::debug!("Weight: {:.2}", scale.get_weight());
                        log::debug!("Flow: {:.2}", scale.get_flow());
                        log::debug!("Level: {}", *level.distance.read().unwrap());
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
                            ambient_probe.clone(),
                            system.config.read().unwrap().boiler.mpc.auto_tune,
                        );
                    }
                    OperationalState::AutoTuning => {
                        if let Some(res) = auto_tuner.run()? {
                            log::info!("Autotune completed");
                            log::info!("Results: {:?}", res);
                            info!(system, "Autotune Results: {:?}", res);

                            let initial_boiler = auto_tuner.get_model_boiler_temperature();

                            let message = BoilerMessage::UpdateParameters {
                                parameters: res,
                                initial_probe_temperature: boiler_temperature,
                                initial_boiler_temperature: initial_boiler,
                            };

                            boiler.send_message(message);

                            system
                                    .operational_state
                                    .lock()
                                    .unwrap()
                                    .transition(crate::state_machines::operational_fsm::Transitions::AutoTuneComplete)
                                    .expect("Invalid transition :(");
                            loop_interval = Duration::from_millis(1000);
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

            (SystemState::Rebooting(instant), _) => {
                // [ ] Shutdown pump, boiler, and put the countdown on the display
                if instant < std::time::Instant::now() {
                    log::info!("Rebooting");
                    std::process::exit(0);
                }
            }

            (_, _) => {
                log::error!("unhandled state")
            }
        }

        let current_state = switches.get_state();
        let level_sensor_max = system
            .config
            .read()
            .unwrap()
            .level_sensor
            .low_level_threshold;

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
                    level.send_message(sensors::a02yyuw::Message::DoRead);
                    std::thread::sleep(Duration::from_millis(400));
                    if *level.distance.read().unwrap() >= level_sensor_max {
                        log::warn!("Threshold too low to brew");
                    } else {
                        info!(system, "Switched to brew");
                        log::info!("Switched to brew");
                        system.board.scale.start_brew();
                        pump.turn_on(Some(Duration::from_secs(5)));
                        let mode = components::boiler::Mode::Mpc { target: 94.0 };
                        boiler.send_message(BoilerMessage::SetMode(mode));
                    }
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
