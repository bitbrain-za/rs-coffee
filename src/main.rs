use anyhow::Result;
use esp_idf_svc::hal::adc::oneshot::AdcDriver;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::PinDriver;
use esp_idf_svc::hal::prelude::Peripherals;
mod board;
// mod thermostat;
mod boiler_pid;
mod mock_boiler;

fn main() -> Result<()> {
    dotenv::dotenv().ok();
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Hello, world!");

    let peripherals = Peripherals::take().unwrap();

    // let mut led_solenoid = PinDriver::output(peripherals.pins.gpio14)?;
    // let mut led_pump = PinDriver::output(peripherals.pins.gpio13)?;
    // let mut solenoid = board::Solenoid::new(peripherals.pins.gpio14)?;

    let adc = AdcDriver::new(peripherals.adc2)?;

    let mut boiler =
        boiler_pid::BoilerPid::new(adc, peripherals.pins.gpio11, peripherals.pins.gpio12, 60.0)?;

    loop {
        boiler.poll()?;
        FreeRtos::delay_ms(100);
    }
}
