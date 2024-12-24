// [ ] Go through all the "expects" and change them to put the system into an error/panic state
// [ ] Remove this later, just silence warnings while we're doing large scale writing
// #![allow(dead_code)]
mod app_state;
mod board;
mod components;
mod config;
mod gpio;
mod indicator;
mod kv_store;
mod models;
mod sensors;
mod state_machines;
use anyhow::Result;
use app_state::System;
use board::{Action, F32Read, Reading};
use state_machines::operational_fsm::OperationalState;
use state_machines::system_fsm::{SystemState, Transition as SystemTransition};
use std::thread;
use std::time::Duration;

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

    #[cfg(feature = "simulate")]
    {
        log::info!("Running simulation");
        let mut auto_tuner =
            models::auto_tune::HeuristicAutoTuner::new(Duration::from_millis(1000));
        if let Err(e) = auto_tuner.simulate() {
            log::error!("{:?}", e);
        }
    }
    loop {
        thread::sleep(Duration::from_secs(1));
    }

    let (system, element) = System::new();
    {
        let board = system.board.lock().unwrap();
        *board.outputs.boiler_duty_cycle.lock().unwrap() = 0.5;
        *board.outputs.pump_duty_cycle.lock().unwrap() = 0.2;
        *board.outputs.solenoid.lock().unwrap() =
            gpio::relay::State::on(Some(Duration::from_secs(5)));
    }

    system
        .system_state
        .lock()
        .unwrap()
        .transition(SystemTransition::Idle)
        .expect("Invalid transition :(");

    let mut boiler = components::boiler::Boiler::new(system.clone());
    boiler.set(components::boiler::Mode::BangBang {
        upper_threshold: 95.0,
        lower_threshold: 85.0,
    });
    boiler.start(element);

    let mut loop_interval = Duration::from_millis(1000);
    loop {
        let system_state = system.system_state.lock().unwrap().clone();
        let operational_state = system.operational_state.lock().unwrap().clone();

        match (system_state, operational_state) {
            (SystemState::Healthy, operational_state) => {
                let boiler_temperature = system.read_f32(F32Read::BoilerTemperature);
                let pump_pressure = system.read_f32(F32Read::PumpPressure);
                log::debug!("Boiler temperature: {}", boiler_temperature);
                log::debug!("Pump pressure: {}", pump_pressure);
                log::debug!("Weight: {}", system.read_f32(F32Read::ScaleWeight));

                match operational_state {
                    OperationalState::Idle => {
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
                    OperationalState::AutoTuning(_, _) => {
                        log::info!("Autotuning in progress");
                        log::info!("{}", operational_state);

                        /* We have to run the device for a bit and collect it's dynamics
                           once that's expired, we do a curve fit to get the parameters
                           for the boiler model.

                           Step 1. Run for an hour collecting data
                           Step 2. Fit curve
                           Step 3. Update parameters
                        */
                        loop_interval = Duration::from_secs(1);

                        let indicator =
                            if let Some(percentage) = operational_state.percentage_complete() {
                                log::info!("Autotuning is {:.2}% complete", percentage);
                                indicator::ring::State::Guage {
                                    min: 0.0,
                                    max: 100.0,
                                    level: percentage,
                                }
                            } else {
                                log::error!("Couldn't get percentage completed");
                                indicator::ring::State::Temperature {
                                    min: 25.0,
                                    max: 150.0,
                                    level: boiler_temperature,
                                }
                            };
                        let _ = system.execute_board_action(Action::SetIndicator(indicator));
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
                    }
                    if button == board::ButtonEnum::HotWater {
                        boiler.set(components::boiler::Mode::BangBang {
                            upper_threshold: 95.0,
                            lower_threshold: 85.0,
                        });
                    }
                    if button == board::ButtonEnum::Steam {
                        boiler.set(components::boiler::Mode::Off);
                    }
                }
            }
        }
        thread::sleep(loop_interval);
    }
}
