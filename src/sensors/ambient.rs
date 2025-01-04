use crate::types::Temperature;
use ds18b20::{Ds18b20, Resolution};
use esp_idf_hal::delay::Delay;
use esp_idf_hal::gpio::PinDriver;
use esp_idf_hal::{
    gpio::{InputPin, OutputPin},
    peripheral::Peripheral,
};
use one_wire_bus::OneWire;
use std::sync::{Arc, RwLock};

pub struct AmbientSensor {
    pub temperature: Arc<RwLock<Temperature>>,
}

impl AmbientSensor {
    pub fn new(one_wire_pin: impl Peripheral<P = impl OutputPin + InputPin> + 'static) -> Self {
        const GUESS_AT_AMBIENT_TEMP: Temperature = 25.0;
        let temperature_probe = Arc::new(RwLock::new(GUESS_AT_AMBIENT_TEMP));
        let temperature_probe_clone = temperature_probe.clone();

        let mut delay = Delay::default();
        let one_wire_pin = PinDriver::input_output_od(one_wire_pin).unwrap();
        let mut one_wire_bus = OneWire::new(one_wire_pin).unwrap();

        std::thread::spawn(move || {
            #[cfg(feature = "simulate")]
            loop {
                std::thread::sleep(std::time::Duration::from_secs(5));
                *temperature_probe_clone.write().unwrap() = GUESS_AT_AMBIENT_TEMP;
            }
            let mut devices = 0;
            while devices == 0 {
                for device_address in one_wire_bus.devices(false, &mut delay) {
                    match device_address {
                        Ok(device_address) => {
                            log::info!(
                                "Found device at address {:?} with family code: {:#x?}",
                                device_address,
                                device_address.family_code()
                            );
                            devices += 1;
                        }
                        Err(e) => {
                            log::error!("Error while searching for devices: {:?}", e);
                            break;
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(2));
            }

            ds18b20::start_simultaneous_temp_measurement(&mut one_wire_bus, &mut delay).unwrap();
            Resolution::Bits12.delay_for_measurement_time(&mut delay);

            let mut search_state = None;
            let sensor = loop {
                match one_wire_bus.device_search(search_state.as_ref(), false, &mut delay) {
                    Ok(Some((device_address, state))) => {
                        search_state = Some(state);
                        if device_address.family_code() != ds18b20::FAMILY_CODE {
                            log::warn!("Device at {:?} has incorrect family code", device_address);
                            continue;
                        }
                        let sensor: Ds18b20 = Ds18b20::new::<String>(device_address).unwrap();

                        match sensor.read_data(&mut one_wire_bus, &mut delay) {
                            Ok(sensor_data) => {
                                *temperature_probe_clone.write().unwrap() = sensor_data.temperature;
                                log::info!(
                                    "Device at {:?} is {}°C",
                                    device_address,
                                    sensor_data.temperature
                                );
                            }
                            Err(e) => {
                                log::warn!("Error reading data from device: {:?}", e);
                            }
                        }
                        /* Just grab the first one, there shouldn't be two */
                        break sensor;
                    }
                    Ok(None) => {
                        log::warn!("No more devices found");
                        ds18b20::start_simultaneous_temp_measurement(&mut one_wire_bus, &mut delay)
                            .unwrap();
                        Resolution::Bits12.delay_for_measurement_time(&mut delay);
                    }
                    Err(e) => {
                        log::warn!("Error searching for devices: {:?}", e);
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(5));
            };

            loop {
                std::thread::sleep(std::time::Duration::from_secs(5));
                match sensor.start_temp_measurement(&mut one_wire_bus, &mut delay) {
                    Ok(_) => {
                        Resolution::Bits12.delay_for_measurement_time(&mut delay);
                        match sensor.read_data(&mut one_wire_bus, &mut delay) {
                            Ok(sensor_data) => {
                                *temperature_probe_clone.write().unwrap() = sensor_data.temperature;
                                log::debug!(
                                    "Device at {:?} is {}°C",
                                    sensor.address(),
                                    sensor_data.temperature
                                );
                            }
                            Err(e) => {
                                log::warn!("Error reading data from device: {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Error starting temperature measurement: {:?}", e);
                    }
                }
            }
        });

        Self {
            temperature: temperature_probe,
        }
    }
}
