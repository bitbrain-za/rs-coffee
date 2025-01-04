use crate::components::{boiler::Boiler, pump::Pump};
use crate::config::Config;
use crate::gpio::{adc::Adc, switch::Switches};
use crate::indicator::ring::{Ring, State as IndicatorState};
use crate::schemas::status::Device as DeviceReport;
use crate::sensors::a02yyuw::A02yyuw;
use crate::sensors::pressure::SeeedWaterPressureSensor;
use crate::sensors::pt100::Pt100;
use crate::sensors::scale::{Interface as LoadCell, Scale};
use crate::sensors::traits::TemperatureProbe;
use crate::state_machines::{
    operational_fsm::{OperationalState, Transitions},
    ArcMutexState,
};
use core::convert::TryInto;
use embedded_svc::wifi::{AuthMethod, ClientConfiguration, Configuration};
use esp_idf_hal::adc::{
    attenuation,
    oneshot::{config::AdcChannelConfig, AdcChannelDriver, AdcDriver},
};
use esp_idf_svc::hal::task::block_on;
use esp_idf_svc::hal::{delay::FreeRtos, prelude::Peripherals};
use esp_idf_svc::timer::EspTaskTimerService;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    nvs::EspDefaultNvsPartition,
    wifi::{AsyncWifi, EspWifi},
};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;

#[derive(Clone)]
pub struct Board {
    pub indicator: Ring,
    pub onboard_rgb: Ring,
    pub temperature: Arc<RwLock<f32>>,
    pub ambient_temperature: Arc<RwLock<f32>>,
    pub scale: LoadCell,
    pub switches: Switches,
    pub pressure: Arc<RwLock<f32>>,
    pub pump: Pump,
    pub boiler: Boiler,
    pub level_sensor: A02yyuw,
}

impl Board {
    pub fn new(operational_state: Arc<Mutex<OperationalState>>, config: &Config) -> Self {
        operational_state
            .transition(Transitions::StartingUpStage("Board Setup".to_string()))
            .expect("Failed to set operational state");

        let peripherals = Peripherals::take().expect("You're probably calling this twice!");

        log::info!("Setting up indicator");
        operational_state
            .transition(Transitions::StartingUpStage("Indicator Setup".to_string()))
            .expect("Failed to set operational state");

        let onboard_led = Ring::new(
            peripherals.rmt.channel0,
            peripherals.pins.gpio48,
            config.indicator.refresh_interval,
            1,
        );
        onboard_led.set_state(IndicatorState::Heartbeat);

        let led_pin = peripherals.pins.gpio21;
        let channel = peripherals.rmt.channel1;

        let ring = Ring::new(
            channel,
            led_pin,
            config.indicator.refresh_interval,
            config.indicator.led_count,
        );
        ring.set_state(IndicatorState::Busy);

        operational_state
            .transition(Transitions::StartingUpStage("Input Setup".to_string()))
            .expect("Failed to set operational state");

        let ambient_probe = crate::sensors::ambient::AmbientSensor::new(peripherals.pins.gpio3);

        log::info!("Setting up wifi");
        let sys_loop = EspSystemEventLoop::take().expect("Unable to take sysloop");
        let timer_service = EspTaskTimerService::new().expect("Failed to create timer service");
        let nvs = EspDefaultNvsPartition::take().expect("Failed to take nvs partition");

        let mut wifi = AsyncWifi::wrap(
            EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))
                .expect("Failed to create wifi"),
            sys_loop,
            timer_service,
        )
        .expect("Failed to create async wifi");
        match block_on(Self::connect_wifi(&mut wifi)) {
            Ok(_) => {
                let ip_info = wifi
                    .wifi()
                    .sta_netif()
                    .get_ip_info()
                    .expect("Failed to get IP info");
                log::info!("Wifi DHCP info: {:?}", ip_info);
            }
            Err(e) => log::error!("Failed to connect wifi: {:?}", e),
        }
        core::mem::forget(wifi);

        log::info!("Setting up switches");
        let switches = Switches::new(
            peripherals.pins.gpio6,
            peripherals.pins.gpio7,
            peripherals.pins.gpio15,
        );

        log::info!("Setting up ADCs");
        let pressure_probe = Arc::new(RwLock::new(0.0));
        let temperature = Arc::new(RwLock::new(f32::default()));
        #[cfg(not(feature = "simulate"))]
        let temperature_clone = temperature.clone();
        let pressure_probe_clone = pressure_probe.clone();

        let seed_pressure_probe = SeeedWaterPressureSensor::default();
        let pt100 = Pt100 {
            calibration: config.boiler.pt100_calibration_factor,
        };

        log::info!("Setting up scale");
        let dt = peripherals.pins.gpio36;
        let sck = peripherals.pins.gpio35;
        let loadcell = Scale::start(sck, dt, &config.load_cell).unwrap();

        log::info!("Setting up level sensor");
        let tx = peripherals.pins.gpio43;
        let rx = peripherals.pins.gpio44;
        let uart = peripherals.uart0;
        let level_sensor = A02yyuw::new(uart, rx, tx);
        log::info!("Starting level sensor");

        let sensor_killswitch = Arc::new(Mutex::new(false));
        let sensor_killswitch_clone = sensor_killswitch.clone();
        let adc_polling_interval = config.adc.polling_interval;
        let adc_window = config.adc.window;
        thread::Builder::new()
            .name("sensor".to_string())
            .spawn(move || {
                let adc = AdcDriver::new(peripherals.adc1).expect("Failed to create ADC driver");
                let channel_config = AdcChannelConfig {
                    attenuation: attenuation::DB_11,
                    calibration: true,
                    ..Default::default()
                };

                let temperature_probe =
                    AdcChannelDriver::new(&adc, peripherals.pins.gpio4, &channel_config)
                        .expect("Failed to create ADC channel temperature");
                let pressure_probe =
                    AdcChannelDriver::new(&adc, peripherals.pins.gpio5, &channel_config)
                        .expect("Failed to create ADC channel pressure");
                let mut adc = Adc::new(
                    temperature_probe,
                    pressure_probe,
                    adc_polling_interval,
                    adc_window,
                );

                loop {
                    if *sensor_killswitch_clone.lock().unwrap() {
                        log::info!("Sensor thread killed");
                        return;
                    }
                    if let Some((temperature, pressure)) = adc.read() {
                        let degrees = match pt100.convert_voltage_to_degrees(temperature / 1000.0) {
                            Ok(degrees) => degrees,
                            Err(e) => {
                                log::error!("Failed to convert voltage to degrees: {:?}", e);
                                999.0
                            }
                        };
                        #[cfg(not(feature = "simulate"))]
                        {
                            *temperature_clone.write().unwrap() = degrees;
                        }
                        #[cfg(feature = "simulate")]
                        {
                            let _ = degrees;
                        }
                        use crate::sensors::traits::PressureProbe;
                        let pressure = match seed_pressure_probe
                            .convert_voltage_to_pressure(pressure / 1000.0)
                        {
                            Ok(pressure) => pressure,
                            Err(e) => {
                                log::error!("Failed to convert voltage to pressure: {:?}", e);
                                continue;
                            }
                        };
                        *pressure_probe_clone.write().unwrap() = pressure;
                    }

                    FreeRtos::delay_ms(10);
                }
            })
            .expect("Failed to spawn sensor thread");

        operational_state
            .transition(Transitions::StartingUpStage("Output Setup".to_string()))
            .expect("Failed to set operational state");
        log::info!("Setting up outputs");

        let boiler = Boiler::new(
            ambient_probe.temperature.clone(),
            temperature.clone(),
            peripherals.pins.gpio1,
            config.boiler,
        );
        let pump = Pump::new(
            peripherals.pins.gpio42,
            peripherals.pins.gpio2,
            pressure_probe.clone(),
            loadcell.weight.clone(),
            config.pump,
        );

        log::info!("Board setup complete");

        Board {
            indicator: ring,
            onboard_rgb: onboard_led,
            temperature,
            ambient_temperature: ambient_probe.temperature,
            scale: loadcell,
            switches,
            pump,
            boiler,
            pressure: pressure_probe,
            level_sensor,
        }
    }

    async fn connect_wifi(wifi: &mut AsyncWifi<EspWifi<'static>>) -> anyhow::Result<()> {
        use dotenv_codegen::dotenv;
        let wifi_configuration = Configuration::Client(ClientConfiguration {
            ssid: dotenv!("WIFI_SSID")
                .try_into()
                .expect("Failed to parse SSID"),
            auth_method: AuthMethod::None,
            password: dotenv!("WIFI_PASSWORD")
                .try_into()
                .expect("Failed to parse password"),
            ..Default::default()
        });

        wifi.set_configuration(&wifi_configuration)?;

        wifi.start().await?;
        log::info!("Wifi started");

        wifi.connect().await?;
        log::info!("Wifi connected");

        wifi.wait_netif_up().await?;
        log::info!("Wifi netif up");

        Ok(())
    }

    pub fn generate_report(&self) -> DeviceReport {
        DeviceReport {
            temperature: *self.temperature.read().unwrap(),
            pressure: *self.pressure.read().unwrap(),
            weight: *self.scale.weight.read().unwrap(),
            ambient: *self.ambient_temperature.read().unwrap(),
            level: *self.level_sensor.distance.read().unwrap(),
            power: 0.0,
        }
    }
}
