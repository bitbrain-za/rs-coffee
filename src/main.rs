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
use sensors::scale::Scale;
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Starting up");

    // let peripherals = Peripherals::take().unwrap();
    let system = System::new();

    let mut board = Board::new();
    board.indicators.set_state(indicator::ring::State::Busy);

    // log::info!("Setting up scale");
    // let dt = peripherals.pins.gpio36;
    // let sck = peripherals.pins.gpio35;
    // let system_scale = system.clone();
    // let mut scale = Scale::new(
    //     sck,
    //     dt,
    //     config::LOAD_SENSOR_SCALING,
    //     config::SCALE_POLLING_RATE_MS,
    //     system_scale,
    //     config::SCALE_SAMPLES,
    // )
    // .unwrap();

    // scale.tare(32);

    // while !scale.is_ready() {
    //     FreeRtos::delay_ms(100);
    // }

    // let system_adc = system.clone();
    // // Sensor Thread
    // std::thread::spawn(move || {
    //     let adc = AdcDriver::new(peripherals.adc1).expect("Failed to create ADC driver");
    //     let config = AdcChannelConfig {
    //         attenuation: attenuation::DB_11,
    //         calibration: true,
    //         ..Default::default()
    //     };

    //     let temperature_probe = AdcChannelDriver::new(&adc, peripherals.pins.gpio4, &config)
    //         .expect("Failed to create ADC channel temperature");
    //     let pressure_probe = AdcChannelDriver::new(&adc, peripherals.pins.gpio5, &config)
    //         .expect("Failed to create ADC channel pressure");

    //     let mut adc = Adc::new(
    //         temperature_probe,
    //         pressure_probe,
    //         config::ADC_POLLING_RATE_MS,
    //         config::ADC_SAMPLES,
    //         system_adc,
    //     );
    //     loop {
    //         let next_tick: Vec<Duration> = vec![adc.poll(), scale.poll()];
    //         FreeRtos::delay_ms(
    //             next_tick
    //                 .iter()
    //                 .min()
    //                 .unwrap_or(&Duration::from_millis(100))
    //                 .as_millis() as u32,
    //         );
    //     }
    // });

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
    system.set_boiler_duty_cycle(0.5);
    system.set_pump_duty_cycle(1.0);
    system.solenoid_turn_on(Some(Duration::from_secs(5)));

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

        let boiler_temperature = system.get_boiler_temperature();
        let pump_pressure = system.get_pump_pressure();
        println!("Boiler temperature: {}", boiler_temperature);
        println!("Pump pressure: {}", pump_pressure);
        println!("Weight: {}", system.get_weight());

        let presses = board.buttons.button_presses();
        if !presses.is_empty() {
            for button in presses {
                println!("Button pressed: {}", button);
            }
        }

        thread::sleep(Duration::from_millis(1000));
    }
    /***********************************************/
}
