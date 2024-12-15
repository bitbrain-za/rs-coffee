use anyhow::Result;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::prelude::Peripherals;
use gpio::adc::Adc;
use std::time::Duration;
mod app_state;
mod gpio;
mod indicator;
mod sensors;
use crate::sensors::scale::Scale;
use app_state::System;
use esp_idf_hal::adc::attenuation::DB_11;
use esp_idf_hal::adc::oneshot::config::AdcChannelConfig;
use esp_idf_hal::adc::oneshot::*;
use esp_idf_hal::gpio::{InterruptType, PinDriver, Pull};
use esp_idf_svc::hal::adc::oneshot::AdcDriver;
use gpio::pwm::PwmBuilder;
use gpio::relay::Relay;
mod config;

fn main() -> Result<()> {
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

    let mut button_brew = PinDriver::input(peripherals.pins.gpio6)?;
    let mut button_steam = PinDriver::input(peripherals.pins.gpio15)?;
    let mut button_hot_water = PinDriver::input(peripherals.pins.gpio7)?;

    button_brew.set_pull(Pull::Down)?;
    button_brew.set_interrupt_type(InterruptType::PosEdge)?;
    button_steam.set_pull(Pull::Down)?;
    button_steam.set_interrupt_type(InterruptType::PosEdge)?;
    button_hot_water.set_pull(Pull::Down)?;
    button_hot_water.set_interrupt_type(InterruptType::PosEdge)?;

    unsafe {
        let system_brew_button = system.clone();
        button_brew
            .subscribe(move || {
                system_brew_button.press_button(app_state::Buttons::Brew);
            })
            .unwrap();

        let system_steam_button = system.clone();
        button_steam
            .subscribe(move || {
                system_steam_button.press_button(app_state::Buttons::Steam);
            })
            .unwrap();

        let system_hot_water_button = system.clone();
        button_hot_water
            .subscribe(move || {
                system_hot_water_button.press_button(app_state::Buttons::HotWater);
            })
            .unwrap();
    }

    let dt = peripherals.pins.gpio36;
    let sck = peripherals.pins.gpio35;
    let system_scale = system.clone();
    let mut scale = Scale::new(
        sck,
        dt,
        config::LOAD_SENSOR_SCALING,
        config::SCALE_POLLING_RATE_MS,
        system_scale,
        config::SCALE_SAMPLES,
    )
    .unwrap();

    scale.tare(32);

    while !scale.is_ready() {
        FreeRtos::delay_ms(100);
    }

    button_brew.enable_interrupt()?;
    button_steam.enable_interrupt()?;
    button_hot_water.enable_interrupt()?;

    let system_adc = system.clone();
    // Sensor Thread
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
            config::ADC_POLLING_RATE_MS,
            config::ADC_SAMPLES,
            system_adc,
        );
        loop {
            let next_tick: Vec<Duration> = vec![adc.poll(), scale.poll()];
            FreeRtos::delay_ms(
                next_tick
                    .iter()
                    .min()
                    .unwrap_or(&Duration::from_millis(100))
                    .as_millis() as u32,
            );
        }
    });

    // GPIO thread
    let system_gpio = system.clone();
    std::thread::spawn(move || {
        let mut boiler = PwmBuilder::new()
            .with_interval(config::BOILER_PWM_PERIOD)
            .with_pin(peripherals.pins.gpio12)
            .build();

        let mut pump = PwmBuilder::new()
            .with_interval(config::PUMP_PWM_PERIOD)
            .with_pin(peripherals.pins.gpio14)
            .build();

        let mut solenoid = Relay::new(peripherals.pins.gpio13, Some(true));

        loop {
            let mut next_tick: Vec<Duration> = vec![config::OUTPUT_POLL_INTERVAL];
            let requested_boiler_duty_cycle = system_gpio.get_boiler_duty_cycle();

            if boiler.get_duty_cycle() != requested_boiler_duty_cycle {
                boiler.set_duty_cycle(requested_boiler_duty_cycle);
            }
            if let Some(duration) = boiler.tick() {
                next_tick.push(duration);
            }

            let requested_pump_duty_cycle = system_gpio.get_pump_duty_cycle();
            if pump.get_duty_cycle() != requested_pump_duty_cycle {
                pump.set_duty_cycle(requested_pump_duty_cycle);
            }
            if let Some(duration) = pump.tick() {
                next_tick.push(duration);
            }

            let requested_solenoid_state = system_gpio.get_solenoid_state();
            if solenoid.state != requested_solenoid_state {
                solenoid.state = requested_solenoid_state;
            }
            if let Some(duration) = solenoid.tick() {
                next_tick.push(duration);
            }

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
        println!("Weight: {}", system.get_weight());

        let presses = system.button_presses();
        if !presses.is_empty() {
            for button in presses {
                println!("Button pressed: {}", button);
            }
            button_brew.enable_interrupt()?;
            button_steam.enable_interrupt()?;
            button_hot_water.enable_interrupt()?;
        }

        FreeRtos::delay_ms(1000);
    }
}
