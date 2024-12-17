mod app_state;
mod config;
mod gpio;
mod indicator;
mod kv_store;
mod models;
mod sensors;
mod system_status;
use anyhow::Result;
use app_state::System;
use esp_idf_hal::adc::{
    attenuation,
    oneshot::{config::AdcChannelConfig, AdcChannelDriver, AdcDriver},
};
use esp_idf_hal::gpio::{InterruptType, PinDriver, Pull};
use esp_idf_svc::hal::{delay::FreeRtos, prelude::Peripherals};
use esp_idf_svc::nvs::*;
use gpio::{adc::Adc, pwm::PwmBuilder, relay::Relay};
use sensors::scale::Scale;
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Starting up");

    let peripherals = Peripherals::take().unwrap();
    let system = System::new();

    log::info!("Setting up indicator");
    let led_pin = peripherals.pins.gpio21;
    let channel = peripherals.rmt.channel0;
    let system_indicator = system.clone();
    std::thread::spawn(move || {
        let mut ring = indicator::ring::Ring::new(
            channel,
            led_pin,
            config::LED_COUNT,
            config::LED_REFRESH_INTERVAL,
        );
        ring.set_state(indicator::ring::State::Busy);

        loop {
            let requested_indicator_state = system_indicator.get_indicator();
            if ring.state != requested_indicator_state {
                ring.set_state(requested_indicator_state);
            }
            thread::sleep(ring.tick());
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

    log::info!("Setting up scale");
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

    log::info!("Setting up buttons");
    button_brew.enable_interrupt()?;
    button_steam.enable_interrupt()?;
    button_hot_water.enable_interrupt()?;

    let system_adc = system.clone();
    // Sensor Thread
    std::thread::spawn(move || {
        let adc = AdcDriver::new(peripherals.adc1).expect("Failed to create ADC driver");
        let config = AdcChannelConfig {
            attenuation: attenuation::DB_11,
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
    log::info!("Setting up Outputs");
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

    log::info!("Setup complete, starting main loop");
    /**************** TEST SECTION  ****************/
    system.set_boiler_duty_cycle(0.5);
    system.set_pump_duty_cycle(1.0);
    system.solenoid_turn_on(Some(Duration::from_secs(5)));

    let mut level = 0.0;
    let mut start = std::time::Instant::now() - std::time::Duration::from_millis(200);
    loop {
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

        thread::sleep(Duration::from_millis(1000));
    }
    /***********************************************/
}

use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, Debug)]
struct StructToBeStored<'a> {
    some_bytes: &'a [u8],
    a_str: &'a str,
    a_number: i16,
}

fn test_nvs() -> anyhow::Result<()> {
    use postcard::{from_bytes, to_vec};
    let nvs_default_partition: EspNvsPartition<NvsDefault> = EspDefaultNvsPartition::take()?;

    let test_namespace = "test_ns";
    let mut nvs = match EspNvs::new(nvs_default_partition, test_namespace, true) {
        Ok(nvs) => {
            log::info!("Got namespace {:?} from default partition", test_namespace);
            nvs
        }
        Err(e) => panic!("Could't get namespace {:?}", e),
    };

    let key_raw_u8 = "test_raw_u8";
    {
        let key_raw_u8_data: &[u8] = &[42];

        match nvs.set_raw(key_raw_u8, key_raw_u8_data) {
            Ok(_) => log::info!("Key updated"),
            // You can find the meaning of the error codes in the output of the error branch in:
            // https://docs.espressif.com/projects/esp-idf/en/latest/esp32/api-reference/error-codes.html
            Err(e) => log::info!("Key not updated {:?}", e),
        };
    }

    {
        let key_raw_u8_data: &mut [u8] = &mut [u8::MAX];

        match nvs.get_raw(key_raw_u8, key_raw_u8_data) {
            Ok(v) => match v {
                Some(vv) => log::info!("{:?} = {:?}", key_raw_u8, vv),
                None => todo!(),
            },
            Err(e) => log::info!("Couldn't get key {} because{:?}", key_raw_u8, e),
        };
    }

    let key_raw_str: &str = "test_raw_str";
    {
        let key_raw_str_data = "Hello from the NVS (I'm raw)!";

        match nvs.set_raw(
            key_raw_str,
            &to_vec::<&str, 100>(&key_raw_str_data).unwrap(),
        ) {
            Ok(_) => log::info!("Key {} updated", key_raw_str),
            Err(e) => log::info!("Key {} not updated {:?}", key_raw_str, e),
        };
    }

    {
        let key_raw_str_data: &mut [u8] = &mut [0; 100];

        match nvs.get_raw(key_raw_str, key_raw_str_data) {
            Ok(v) => {
                if let Some(the_str) = v {
                    log::info!("{:?} = {:?}", key_raw_str, from_bytes::<&str>(the_str));
                }
            }
            Err(e) => log::info!("Couldn't get key {} because {:?}", key_raw_str, e),
        };
    }

    let key_raw_struct: &str = "test_raw_struct";
    {
        let key_raw_struct_data = StructToBeStored {
            some_bytes: &[1, 2, 3, 4],
            a_str: "I'm a str inside a struct!",
            a_number: 42,
        };

        match nvs.set_raw(
            key_raw_struct,
            &to_vec::<StructToBeStored, 100>(&key_raw_struct_data).unwrap(),
        ) {
            Ok(_) => log::info!("Key {} updated", key_raw_struct),
            Err(e) => log::info!("key {} not updated {:?}", key_raw_struct, e),
        };
    }

    {
        let key_raw_struct_data: &mut [u8] = &mut [0; 100];

        match nvs.get_raw(key_raw_struct, key_raw_struct_data) {
            Ok(v) => {
                if let Some(the_struct) = v {
                    log::info!(
                        "{:?} = {:?}",
                        key_raw_struct,
                        from_bytes::<StructToBeStored>(the_struct)
                    )
                }
            }
            Err(e) => log::info!("Couldn't get key {} because {:?}", key_raw_struct, e),
        };
    }

    Ok(())
}
