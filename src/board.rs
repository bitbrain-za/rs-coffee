use crate::config;
use crate::gpio::{
    adc::Adc,
    button::Button,
    pwm::{Pwm, PwmBuilder},
    relay::Relay,
    relay::State as RelayState,
};
use crate::indicator::ring::{Ring, State as IndicatorState};
use crate::sensors::pressure::SeeedWaterPressureSensor;
#[cfg(not(feature = "simulate"))]
use crate::sensors::pt100::Pt100;
use crate::sensors::scale::Scale as LoadCell;
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
use esp_idf_svc::hal::gpio::{Gpio12, Gpio15, Gpio6, Gpio7, InputPin, OutputPin};
use esp_idf_svc::hal::task::block_on;
use esp_idf_svc::hal::{delay::FreeRtos, prelude::Peripherals};
use esp_idf_svc::timer::EspTaskTimerService;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    wifi::{AsyncWifi, EspWifi},
};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub type Element = Pwm<'static, Gpio12>;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ButtonEnum {
    Brew,
    Steam,
    HotWater,
}

impl std::fmt::Display for ButtonEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ButtonEnum::Brew => write!(f, "Brew"),
            ButtonEnum::Steam => write!(f, "Steam"),
            ButtonEnum::HotWater => write!(f, "Hot Water"),
        }
    }
}

pub struct Buttons<'a, PA: InputPin + OutputPin, PB: InputPin + OutputPin, PC: InputPin + OutputPin>
{
    brew_button: Button<'a, PA>,
    steam_button: Button<'a, PB>,
    hot_water_button: Button<'a, PC>,
}

impl<'a, PA, PB, PC> Buttons<'a, PA, PB, PC>
where
    PA: InputPin + OutputPin,
    PB: InputPin + OutputPin,
    PC: InputPin + OutputPin,
{
    pub fn was_button_pressed(&mut self, button: ButtonEnum) -> bool {
        match button {
            ButtonEnum::Brew => self.brew_button.was_pressed(),
            ButtonEnum::Steam => self.steam_button.was_pressed(),
            ButtonEnum::HotWater => self.hot_water_button.was_pressed(),
        }
    }

    pub fn button_presses(&mut self) -> Vec<ButtonEnum> {
        let mut buttons = Vec::new();

        if self.was_button_pressed(ButtonEnum::Brew) {
            buttons.push(ButtonEnum::Brew);
        }
        if self.was_button_pressed(ButtonEnum::Steam) {
            buttons.push(ButtonEnum::Steam);
        }
        if self.was_button_pressed(ButtonEnum::HotWater) {
            buttons.push(ButtonEnum::HotWater);
        }
        buttons
    }
}

pub struct Indicators {
    state: Arc<Mutex<IndicatorState>>,
    handle: thread::JoinHandle<()>,
    kill_switch: Arc<Mutex<bool>>,
}

impl Indicators {
    pub fn set_state(&self, state: IndicatorState) {
        *self.state.lock().unwrap() = state;
    }

    pub fn kill(&self) {
        *self.kill_switch.lock().unwrap() = true;
    }
}

pub struct Scale {
    weight: Arc<Mutex<f32>>,
}

impl Scale {
    pub fn get_weight(&self) -> f32 {
        *self.weight.lock().unwrap()
    }
    pub fn set_weight(&self, weight: f32) {
        *self.weight.lock().unwrap() = weight;
    }
}

#[derive(Default)]
pub struct Temperature {
    #[cfg(not(feature = "simulate"))]
    degrees: Arc<Mutex<f32>>,
    #[cfg(feature = "simulate")]
    pub degrees: Arc<Mutex<f32>>,
}

impl Temperature {
    pub fn get_temperature(&self) -> f32 {
        *self.degrees.lock().unwrap()
    }
    #[cfg(feature = "simulate")]
    pub fn set_temperature(&self, degrees: f32) {
        *self.degrees.lock().unwrap() = degrees;
    }
    #[cfg(not(feature = "simulate"))]
    pub fn set_temperature(
        &self,
        voltage: f32,
        probe: impl crate::sensors::traits::TemperatureProbe,
    ) {
        {
            *self.degrees.lock().unwrap() = probe
                .convert_voltage_to_degrees(voltage)
                .expect("Failed to convert voltage to degrees");
        }
    }
}

#[derive(Default)]
pub struct Pressure {
    pressure: f32,
}

impl Pressure {
    pub fn get_pressure(&self) -> f32 {
        self.pressure
    }
    pub fn set_pressure(
        &mut self,
        voltage: f32,
        probe: impl crate::sensors::traits::PressureProbe,
    ) {
        self.pressure = probe
            .convert_voltage_to_pressure(voltage)
            .expect("Failed to convert voltage to pressure");
    }
}

pub struct Sensors<'a> {
    pub buttons: Buttons<'a, Gpio6, Gpio15, Gpio7>,
    pub scale: Scale,
    pub temperature: Arc<Mutex<Temperature>>,
    pub pressure: Arc<Mutex<Pressure>>,
    handle: thread::JoinHandle<()>,
    kill_switch: Arc<Mutex<bool>>,
}

impl Sensors<'_> {
    pub fn kill(&self) {
        *self.kill_switch.lock().unwrap() = true;
    }
}

pub struct Outputs {
    pub boiler_duty_cycle: Arc<Mutex<f32>>,
    pub pump_duty_cycle: Arc<Mutex<f32>>,
    pub solenoid: Arc<Mutex<RelayState>>,
    handle: thread::JoinHandle<()>,
    kill_switch: Arc<Mutex<bool>>,
}

impl Outputs {
    pub fn kill(&self) {
        *self.kill_switch.lock().unwrap() = true;
    }
}

pub struct Board<'a> {
    pub indicators: Indicators,
    pub sensors: Sensors<'a>,
    pub outputs: Outputs,
}

impl<'a> Board<'a> {
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

        let indicator_ring = Arc::new(Mutex::new(IndicatorState::Busy));
        let indicator_killswitch = Arc::new(Mutex::new(false));

        let indicator_ring_clone = indicator_ring.clone();
        let indicator_killswitch_clone = indicator_killswitch.clone();

        let indicator_handle = thread::Builder::new()
            .name("indicator".to_string())
            .spawn(move || {
                let mut ring = Ring::new(
                    channel,
                    led_pin,
                    config::LED_COUNT,
                    config::LED_REFRESH_INTERVAL,
                );
                ring.set_state(IndicatorState::Busy);

                log::info!("Starting indicator thread");
                loop {
                    if *indicator_killswitch_clone.lock().unwrap() {
                        ring.set_state(IndicatorState::Off);
                        log::info!("Indicator thread killed");
                        return;
                    }
                    let requested_indicator_state = *indicator_ring_clone.lock().unwrap();
                    if ring.state != requested_indicator_state {
                        ring.set_state(requested_indicator_state);
                    }
                    thread::sleep(ring.tick());
                }
            })
            .expect("Failed to spawn indicator thread");

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

        log::info!("Setting up buttons");
        let mut button_brew = Button::new(peripherals.pins.gpio6, None);
        let mut button_steam = Button::new(peripherals.pins.gpio15, None);
        let mut button_hot_water = Button::new(peripherals.pins.gpio7, None);

        button_brew.enable();
        button_steam.enable();
        button_hot_water.enable();

        log::info!("Setting up ADCs");
        let pressure = Arc::new(Mutex::new(Pressure::default()));
        let temperature = Arc::new(Mutex::new(Temperature::default()));
        let pressure_clone = pressure.clone();
        #[cfg(not(feature = "simulate"))]
        let temperature_clone = temperature.clone();

        use crate::kv_store::Storable;
        let seed_pressure_probe = SeeedWaterPressureSensor::load_or_default();
        #[cfg(not(feature = "simulate"))]
        let pt100 = Pt100::load_or_default();

        log::info!("Setting up scale");
        let dt = peripherals.pins.gpio36;
        let sck = peripherals.pins.gpio35;
        let weight = Arc::new(Mutex::new(0.0));
        let mut loadcell = LoadCell::new(
            sck,
            dt,
            config::LOAD_SENSOR_SCALING,
            config::SCALE_POLLING_RATE_MS,
            config::SCALE_SAMPLES,
        )
        .unwrap();
        loadcell.tare(32);

        while !loadcell.is_ready() {
            std::thread::sleep(Duration::from_millis(100));
        }

        let sensor_killswitch = Arc::new(Mutex::new(false));
        let sensor_killswitch_clone = sensor_killswitch.clone();
        let weight_clone = weight.clone();
        let sensor_handle = thread::Builder::new()
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

                let mut poll_counter = 10;

                loop {
                    if *sensor_killswitch_clone.lock().unwrap() {
                        log::info!("Sensor thread killed");
                        return;
                    }

                    // Only poll scale every 100ms
                    if poll_counter == 10 {
                        poll_counter = 0;
                        if let Some(reading) = loadcell.read() {
                            *weight_clone.lock().unwrap() = reading;
                        }
                    }

                    #[cfg(feature = "simulate")]
                    if let Some((_, pressure)) = adc.read() {
                        pressure_clone
                            .lock()
                            .unwrap()
                            .set_pressure(pressure, seed_pressure_probe);
                    }

                    #[cfg(not(feature = "simulate"))]
                    if let Some((temperature, pressure)) = adc.read() {
                        temperature_clone
                            .lock()
                            .unwrap()
                            .set_temperature(temperature, pt100);
                        pressure_clone
                            .lock()
                            .unwrap()
                            .set_pressure(pressure, seed_pressure_probe);
                    }

                    FreeRtos::delay_ms(10);
                }
            })
            .expect("Failed to spawn sensor thread");

        operational_state
            .transition(Transitions::StartingUpStage("Output Setup".to_string()))
            .expect("Failed to set operational state");
        log::info!("Setting up outputs");

        let boiler_duty_cycle = Arc::new(Mutex::new(0.0));
        let pump_duty_cycle = Arc::new(Mutex::new(0.0));
        let pump_duty_cycle_clone = pump_duty_cycle.clone();
        let solenoid_state = Arc::new(Mutex::new(RelayState::Off));
        let solenoid_state_clone = solenoid_state.clone();
        let outputs_killswitch = Arc::new(Mutex::new(false));
        let outputs_killswitch_clone = outputs_killswitch.clone();

        let element: Element = PwmBuilder::new()
            .with_interval(config::BOILER_PWM_PERIOD)
            .with_pin(peripherals.pins.gpio12)
            .build();

        let output_thread_handle = std::thread::Builder::new()
            .name("Outputs".to_string())
            .spawn(move || {
                let mut pump = PwmBuilder::new()
                    .with_interval(config::PUMP_PWM_PERIOD)
                    .with_pin(peripherals.pins.gpio14)
                    .build();

                let mut solenoid = Relay::new(peripherals.pins.gpio13, Some(true));

                loop {
                    if *outputs_killswitch_clone.lock().unwrap() {
                        log::info!("Outputs thread killed");
                        return;
                    }
                    let mut next_tick: Vec<Duration> = vec![config::OUTPUT_POLL_INTERVAL];

                    let requested_pump_duty_cycle = *pump_duty_cycle_clone.lock().unwrap();
                    if pump.get_duty_cycle() != requested_pump_duty_cycle {
                        pump.set_duty_cycle(requested_pump_duty_cycle);
                    }
                    if let Some(duration) = pump.tick() {
                        next_tick.push(duration);
                    }

                    let requested_solenoid_state = *solenoid_state_clone.lock().unwrap();
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
            })
            .expect("Failed to spawn output thread");

        (
            Board {
                indicators: Indicators {
                    state: indicator_ring,
                    handle: indicator_handle,
                    kill_switch: indicator_killswitch,
                },
                sensors: Sensors {
                    buttons: Buttons {
                        brew_button: button_brew,
                        steam_button: button_steam,
                        hot_water_button: button_hot_water,
                    },
                    scale: Scale { weight },
                    handle: sensor_handle,
                    kill_switch: sensor_killswitch,
                    temperature,
                    pressure,
                },
                outputs: Outputs {
                    boiler_duty_cycle,
                    pump_duty_cycle,
                    solenoid: solenoid_state,
                    handle: output_thread_handle,
                    kill_switch: outputs_killswitch,
                },
            },
            element,
        )
    }

    pub fn open_valve(&self, duration: Option<Duration>) {
        *self.outputs.solenoid.lock().unwrap() = RelayState::on(duration);
    }
    pub fn close_valve(&self, duration: Option<Duration>) {
        *self.outputs.solenoid.lock().unwrap() = RelayState::off(duration);
    }

    async fn connect_wifi(wifi: &mut AsyncWifi<EspWifi<'static>>) -> anyhow::Result<()> {
        const SSID: &str = "Wokwi-GUEST";
        const PASSWORD: &str = "";
        let wifi_configuration = Configuration::Client(ClientConfiguration {
            ssid: SSID.try_into().unwrap(),
            auth_method: AuthMethod::None,
            password: PASSWORD.try_into().unwrap(),
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
    SetBoilerDutyCycle(f32),
    SetPumpDutyCycle(f32),
    OpenValve(Option<Duration>),
    CloseValve(Option<Duration>),
    SetIndicator(IndicatorState),
    Panic,
    Error,
}

impl Action {
    pub fn execute(&self, board: Arc<Mutex<Board>>) {
        let board = board.lock().unwrap();
        match self {
            Action::SetBoilerDutyCycle(duty_cycle) => {
                *board.outputs.boiler_duty_cycle.lock().unwrap() = *duty_cycle;
            }
            Action::SetPumpDutyCycle(duty_cycle) => {
                *board.outputs.pump_duty_cycle.lock().unwrap() = *duty_cycle;
            }
            Action::OpenValve(duration) => {
                board.open_valve(*duration);
            }
            Action::CloseValve(duration) => {
                board.close_valve(*duration);
            }
            Action::SetIndicator(state) => {
                board.indicators.set_state(*state);
            }
            Action::Error => {
                board.indicators.set_state(IndicatorState::Error);
            }
            Action::Panic => {
                board.indicators.set_state(IndicatorState::Panic);
            }
        }
    }
}

pub enum Reading {
    BoilerTemperature(Option<f32>),
    PumpPressure(Option<f32>),
    ScaleWeight(Option<f32>),
    BrewSwitchState(Option<bool>),
    SteamSwitchState(Option<bool>),
    HotWaterSwitchState(Option<bool>),
    AllButtonsState(Option<Vec<ButtonEnum>>),
}

impl Reading {
    pub fn get(&self, board: Arc<Mutex<Board>>) -> Self {
        let mut board = board.lock().unwrap();
        match self {
            Reading::BoilerTemperature(_) => Reading::BoilerTemperature(Some(
                board.sensors.temperature.lock().unwrap().get_temperature(),
            )),
            Reading::PumpPressure(_) => {
                Reading::PumpPressure(Some(board.sensors.pressure.lock().unwrap().get_pressure()))
            }
            Reading::ScaleWeight(_) => Reading::ScaleWeight(Some(board.sensors.scale.get_weight())),
            Reading::BrewSwitchState(_) => {
                Reading::BrewSwitchState(Some(board.sensors.buttons.brew_button.was_pressed()))
            }
            Reading::SteamSwitchState(_) => {
                Reading::SteamSwitchState(Some(board.sensors.buttons.steam_button.was_pressed()))
            }
            Reading::HotWaterSwitchState(_) => Reading::HotWaterSwitchState(Some(
                board.sensors.buttons.hot_water_button.was_pressed(),
            )),
            Reading::AllButtonsState(_) => {
                Reading::AllButtonsState(Some(board.sensors.buttons.button_presses()))
            }
        }
    }
}

pub enum F32Read {
    BoilerTemperature,
    PumpPressure,
    ScaleWeight,
    PumpDutyCycle,
}

impl F32Read {
    pub fn get(&self, board: Arc<Mutex<Board>>) -> f32 {
        let board = board.lock().unwrap();
        match self {
            F32Read::BoilerTemperature => {
                board.sensors.temperature.lock().unwrap().get_temperature()
            }
            F32Read::PumpPressure => board.sensors.pressure.lock().unwrap().get_pressure(),
            F32Read::ScaleWeight => board.sensors.scale.get_weight(),
            F32Read::PumpDutyCycle => *board.outputs.pump_duty_cycle.lock().unwrap(),
        }
    }
}

pub enum BoolRead {
    Brew,
    Steam,
    HotWater,
}

impl BoolRead {
    pub fn get(&self, board: Arc<Mutex<Board>>) -> bool {
        let mut board = board.lock().unwrap();
        match self {
            BoolRead::Brew => board.sensors.buttons.brew_button.was_pressed(),
            BoolRead::Steam => board.sensors.buttons.steam_button.was_pressed(),
            BoolRead::HotWater => board.sensors.buttons.hot_water_button.was_pressed(),
        }
    }
}
