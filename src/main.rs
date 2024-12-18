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
mod system_status;
use anyhow::Result;
use app_state::System;
use board::Board;
use esp_idf_hal::adc::{
    attenuation,
    oneshot::{config::AdcChannelConfig, AdcChannelDriver, AdcDriver},
};
use esp_idf_hal::gpio::{InterruptType, PinDriver, Pull};
use esp_idf_svc::hal::{delay::FreeRtos, prelude::Peripherals};
use gpio::{adc::Adc, pwm::PwmBuilder, relay::Relay};
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Starting up");

    let mut board = Board::new();
    board.indicators.set_state(indicator::ring::State::Busy);

    // // GPIO thread
    // log::info!("Setting up Outputs");
    // let system_gpio = system.clone();
    // std::thread::spawn(move || {
    //     let mut boiler = PwmBuilder::new()
    //         .with_interval(config::BOILER_PWM_PERIOD)
    //         .with_pin(peripherals.pins.gpio12)
    //         .build();

    //     let mut pump = PwmBuilder::new()
    //         .with_interval(config::PUMP_PWM_PERIOD)
    //         .with_pin(peripherals.pins.gpio14)
    //         .build();

    //     let mut solenoid = Relay::new(peripherals.pins.gpio13, Some(true));

    //     loop {
    //         let mut next_tick: Vec<Duration> = vec![config::OUTPUT_POLL_INTERVAL];
    //         let requested_boiler_duty_cycle = system_gpio.get_boiler_duty_cycle();

    //         if boiler.get_duty_cycle() != requested_boiler_duty_cycle {
    //             boiler.set_duty_cycle(requested_boiler_duty_cycle);
    //         }
    //         if let Some(duration) = boiler.tick() {
    //             next_tick.push(duration);
    //         }

    //         let requested_pump_duty_cycle = system_gpio.get_pump_duty_cycle();
    //         if pump.get_duty_cycle() != requested_pump_duty_cycle {
    //             pump.set_duty_cycle(requested_pump_duty_cycle);
    //         }
    //         if let Some(duration) = pump.tick() {
    //             next_tick.push(duration);
    //         }

    //         let requested_solenoid_state = system_gpio.get_solenoid_state();
    //         if solenoid.state != requested_solenoid_state {
    //             solenoid.state = requested_solenoid_state;
    //         }
    //         if let Some(duration) = solenoid.tick() {
    //             next_tick.push(duration);
    //         }

    //         FreeRtos::delay_ms(
    //             next_tick
    //                 .iter()
    //                 .min()
    //                 .unwrap_or(&Duration::from_millis(100))
    //                 .as_millis() as u32,
    //         );
    //     }
    // });

    log::info!("Setup complete, starting main loop");
    /**************** TEST SECTION  ****************/
    // system.set_boiler_duty_cycle(0.5);
    // system.set_pump_duty_cycle(1.0);
    // system.solenoid_turn_on(Some(Duration::from_secs(5)));

    let mut level = 0.0;
    let mut start = std::time::Instant::now() - std::time::Duration::from_millis(200);
    loop {
        if start.elapsed() > std::time::Duration::from_millis(200) {
            board.indicators.set_state(indicator::ring::State::Guage {
                min: 0.0,
                max: 100.0,
                level,
            });

            level += 1.0;
            if level > 100.0 {
                level = 0.0;
            }
            start = std::time::Instant::now();
        }

        let boiler_temperature = board.sensors.temperature.lock().unwrap().get_temperature();
        let pump_pressure = board.sensors.pressure.lock().unwrap().get_pressure();
        println!("Boiler temperature: {}", boiler_temperature);
        println!("Pump pressure: {}", pump_pressure);
        println!("Weight: {}", board.sensors.scale.get_weight());

        let presses = board.sensors.buttons.button_presses();
        if !presses.is_empty() {
            for button in presses {
                println!("Button pressed: {}", button);
            }
        }

        thread::sleep(Duration::from_millis(1000));
    }
    /***********************************************/
}
