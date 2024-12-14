use anyhow::Result;
use esp_idf_hal::adc;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::prelude::Peripherals;
use gpio::adc::Adc;
use std::time::Duration;
mod app_state;
mod board;
mod gpio;
mod indicator;
mod sensors;
use app_state::System;
use esp_idf_hal::adc::attenuation::DB_11;
use esp_idf_hal::adc::oneshot::config::AdcChannelConfig;
use esp_idf_hal::adc::oneshot::*;
use esp_idf_svc::hal::adc::oneshot::AdcDriver;
use gpio::pwm::PwmBuilder;
use gpio::relay::Relay;
use sensors::boiler_temperature::BoilerTemperature;

fn main() -> Result<()> {
    dotenv::dotenv().ok();
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();
    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();

    let led_pin = peripherals.pins.gpio21;
    let channel = peripherals.rmt.channel0;

    let system = System::new();
    let system_indicator = system.clone();

    std::thread::spawn(move || {
        let mut ring =
            indicator::ring::Ring::new(channel, led_pin, 32, std::time::Duration::from_millis(100));
        ring.set_state(indicator::ring::State::Busy);

        loop {
            let requested_indicator_state = system_indicator.get_indicator();
            if ring.state != requested_indicator_state {
                ring.set_state(requested_indicator_state);
            }
            let next_tick = ring.tick();
            FreeRtos::delay_ms(next_tick.as_millis() as u32);
        }
    });

    system.set_indicator(indicator::ring::State::Busy);

    let system_adc = system.clone();
    // ADC Thread
    std::thread::spawn(move || {
        let adc = AdcDriver::new(peripherals.adc1).expect("Failed to create ADC driver");
        let config = AdcChannelConfig {
            attenuation: DB_11,
            calibration: true,
            ..Default::default()
        };

        let temperature_probe = AdcChannelDriver::new(&adc, peripherals.pins.gpio4, &config)
            .expect("Failed to create ADC channel temperature");
        let pressure_probe = AdcChannelDriver::new(&adc, peripherals.pins.gpio5, &config)
            .expect("Failed to create ADC channel pressure");

        let mut adc = Adc::new(
            temperature_probe,
            pressure_probe,
            std::time::Duration::from_millis(10),
            system_adc,
        );
        loop {
            let next_tick = adc.poll();
            FreeRtos::delay_ms(next_tick.as_millis() as u32);
        }
    });

    // GPIO thread
    let system_gpio = system.clone();
    std::thread::spawn(move || {
        let mut boiler = PwmBuilder::new()
            .with_interval(std::time::Duration::from_millis(2000))
            .with_pin(peripherals.pins.gpio12)
            .with_poll_rate(std::time::Duration::from_millis(100))
            .build();

        let mut pump = PwmBuilder::new()
            .with_interval(std::time::Duration::from_millis(500))
            .with_pin(peripherals.pins.gpio14)
            .with_poll_rate(std::time::Duration::from_millis(100))
            .build();

        let mut solenoid = Relay::new(
            peripherals.pins.gpio13,
            Some(true),
            std::time::Duration::from_millis(100),
        );

        loop {
            let mut next_tick: Vec<Duration> = Vec::new();
            let requested_boiler_duty_cycle = system_gpio.get_boiler_duty_cycle();

            if boiler.get_duty_cycle() != requested_boiler_duty_cycle {
                boiler.set_duty_cycle(requested_boiler_duty_cycle);
            }
            next_tick.push(boiler.tick());

            let requested_pump_duty_cycle = system_gpio.get_pump_duty_cycle();
            if pump.get_duty_cycle() != requested_pump_duty_cycle {
                pump.set_duty_cycle(requested_pump_duty_cycle);
            }
            next_tick.push(pump.tick());

            let requested_solenoid_state = system_gpio.get_solenoid_state();
            if solenoid.state != requested_solenoid_state {
                solenoid.state = requested_solenoid_state;
            }
            next_tick.push(solenoid.tick());

            FreeRtos::delay_ms(
                next_tick
                    .iter()
                    .min()
                    .unwrap_or(&Duration::from_millis(100))
                    .as_millis() as u32,
            );
        }
    });

    system.set_boiler_duty_cycle(0.5);
    system.set_pump_duty_cycle(1.0);
    system.solenoid_turn_on(Some(Duration::from_secs(5)));

    // just a test loop
    let mut level = 0.0;
    let mut start = std::time::Instant::now() - std::time::Duration::from_millis(200);
    loop {
        // test code for the indicator
        if start.elapsed() > std::time::Duration::from_millis(200) {
            system.set_indicator(indicator::ring::State::Guage {
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

        FreeRtos::delay_ms(1000);
    }
}
