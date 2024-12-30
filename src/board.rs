use crate::config;
use crate::gpio::{
    adc::Adc,
    pwm::{Pwm, PwmBuilder},
    switch::Switches,
};
use crate::indicator::ring::{Ring, State as IndicatorState};
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
use esp_idf_svc::hal::gpio::Gpio1;
use esp_idf_svc::hal::task::block_on;
use esp_idf_svc::hal::{delay::FreeRtos, prelude::Peripherals};
use esp_idf_svc::timer::EspTaskTimerService;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    wifi::{AsyncWifi, EspWifi},
};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;

pub type Element = Pwm<'static, Gpio1>;

pub struct Board {
    indicator: Ring,
    pub temperature: Arc<RwLock<f32>>,
    pub scale: LoadCell,
    pub switches: Switches,
    pub pressure: Arc<RwLock<f32>>,
    pub pump: crate::components::pump::Interface,
}

impl Board {
    pub fn new(operational_state: Arc<Mutex<OperationalState>>) -> (Self, Element) {
        operational_state
            .transition(Transitions::StartingUpStage("Board Setup".to_string()))
            .expect("Failed to set operational state");

        let peripherals = Peripherals::take().expect("You're probably calling this twice!");

        log::info!("Setting up indicator");
        operational_state
            .transition(Transitions::StartingUpStage("Indicator Setup".to_string()))
            .expect("Failed to set operational state");

        let led_pin = peripherals.pins.gpio21;
        let channel = peripherals.rmt.channel0;

        let ring = Ring::new(
            channel,
            led_pin,
            config::LED_REFRESH_INTERVAL,
            config::LED_COUNT,
        );
        ring.set_state(IndicatorState::Busy);

        operational_state
            .transition(Transitions::StartingUpStage("Input Setup".to_string()))
            .expect("Failed to set operational state");

        log::info!("Setting up wifi");
        let sys_loop = EspSystemEventLoop::take().expect("Unable to take sysloop");
        let timer_service = EspTaskTimerService::new().expect("Failed to create timer service");

        let mut wifi = AsyncWifi::wrap(
            EspWifi::new(peripherals.modem, sys_loop.clone(), None).expect("Failed to create wifi"),
            sys_loop,
            timer_service,
        )
        .expect("Failed to create async wifi");
        block_on(Self::connect_wifi(&mut wifi)).expect("Failed to connect wifi");
        let ip_info = wifi
            .wifi()
            .sta_netif()
            .get_ip_info()
            .expect("Failed to get IP info");
        log::info!("Wifi DHCP info: {:?}", ip_info);
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

        use crate::kv_store::Storable;
        let seed_pressure_probe = SeeedWaterPressureSensor::load_or_default();
        let pt100 = Pt100::load_or_default();

        log::info!("Setting up scale");
        let dt = peripherals.pins.gpio36;
        let sck = peripherals.pins.gpio35;
        let loadcell = Scale::start(
            sck,
            dt,
            config::SCALE_POLLING_RATE_MS,
            config::SCALE_SAMPLES,
        )
        .unwrap();

        let sensor_killswitch = Arc::new(Mutex::new(false));
        let sensor_killswitch_clone = sensor_killswitch.clone();
        thread::Builder::new()
            .name("sensor".to_string())
            .spawn(move || {
                let adc = AdcDriver::new(peripherals.adc1).expect("Failed to create ADC driver");
                let config = AdcChannelConfig {
                    attenuation: attenuation::DB_11,
                    calibration: true,
                    ..Default::default()
                };

                let temperature_probe =
                    AdcChannelDriver::new(&adc, peripherals.pins.gpio4, &config)
                        .expect("Failed to create ADC channel temperature");
                let pressure_probe = AdcChannelDriver::new(&adc, peripherals.pins.gpio5, &config)
                    .expect("Failed to create ADC channel pressure");
                let mut adc = Adc::new(
                    temperature_probe,
                    pressure_probe,
                    config::ADC_POLLING_RATE_MS,
                    config::ADC_SAMPLES,
                );

                loop {
                    if *sensor_killswitch_clone.lock().unwrap() {
                        log::info!("Sensor thread killed");
                        return;
                    }
                    if let Some((temperature, pressure)) = adc.read() {
                        let degrees = match pt100.convert_voltage_to_degrees(temperature) {
                            Ok(degrees) => degrees,
                            Err(e) => {
                                log::error!("Failed to convert voltage to degrees: {:?}", e);
                                continue;
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
                        let pressure =
                            match seed_pressure_probe.convert_voltage_to_pressure(pressure) {
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

        let element: Element = PwmBuilder::new()
            .with_interval(config::BOILER_PWM_PERIOD)
            .with_pin(peripherals.pins.gpio1)
            .build();

        let pump = crate::components::pump::Pump::start(
            peripherals.pins.gpio42,
            peripherals.pins.gpio2,
            pressure_probe.clone(),
            loadcell.weight.clone(),
            config::PUMP_PWM_PERIOD,
        );

        log::info!("Board setup complete");

        (
            Board {
                indicator: ring,
                temperature,
                scale: loadcell,
                switches,
                pump,
                pressure: pressure_probe,
            },
            element,
        )
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
}

pub enum Action {
    SetIndicator(IndicatorState),
    Panic,
    Error,
}

impl Action {
    pub fn execute(&self, board: Arc<Mutex<Board>>) {
        let board = board.lock().unwrap();
        match self {
            Action::SetIndicator(state) => {
                board.indicator.set_state(*state);
            }
            Action::Error => {
                board.indicator.set_state(IndicatorState::Error);
            }
            Action::Panic => {
                board.indicator.set_state(IndicatorState::Panic);
            }
        }
    }
}
