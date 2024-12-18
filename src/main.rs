// [ ] Go through all the "expects" and change them to put the system into an error/panic state
// [ ] Remove this later, just silence warnings while we're doing large scale writing
#![allow(dead_code)]
mod app_state;
mod board;
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
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Starting up");

    let system = System::new();

    log::info!("Setup complete, starting main loop");
    /**************** TEST SECTION  ****************/
    {
        let board = system.board.lock().unwrap();
        *board.outputs.boiler_duty_cycle.lock().unwrap() = 0.5;
        *board.outputs.pump_duty_cycle.lock().unwrap() = 0.2;
        *board.outputs.solenoid.lock().unwrap() =
            gpio::relay::State::on(Some(Duration::from_secs(5)));
    }

    let mut level = 0.0;
    let mut start = std::time::Instant::now() - std::time::Duration::from_millis(200);
    loop {
        if start.elapsed() > std::time::Duration::from_millis(200) {
            let indicator = indicator::ring::State::Guage {
                min: 0.0,
                max: 100.0,
                level,
            };
            system.execute_board_action(Action::SetIndicator(indicator));

            level += 1.0;
            if level > 100.0 {
                level = 0.0;
            }
            start = std::time::Instant::now();
        }

        let boiler_temperature = system.read_f32(F32Read::BoilerTemperature);
        let pump_pressure = system.read_f32(F32Read::PumpPressure);
        println!("Boiler temperature: {}", boiler_temperature);
        println!("Pump pressure: {}", pump_pressure);
        println!("Weight: {}", system.read_f32(F32Read::ScaleWeight));

        if let Reading::AllButtonsState(Some(presses)) =
            system.do_board_read(Reading::AllButtonsState(None))
        {
            if !presses.is_empty() {
                for button in presses {
                    println!("Button pressed: {}", button);

                    if button == board::ButtonEnum::Brew {
                        system
                            .execute_board_action(Action::OpenValve(Some(Duration::from_secs(5))));
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(1000));
    }
    /***********************************************/
}
