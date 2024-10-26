use anyhow::Result;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::PinDriver;
use esp_idf_svc::hal::prelude::Peripherals;

fn main() -> Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Hello, world!");

    let peripherals = Peripherals::take().unwrap();

    let mut led_green = PinDriver::output(peripherals.pins.gpio14)?;
    let mut led_blue = PinDriver::output(peripherals.pins.gpio13)?;
    let mut led_red = PinDriver::output(peripherals.pins.gpio12)?;

    loop {
        led_green.set_high()?;
        led_blue.set_low()?;
        led_red.set_low()?;
        FreeRtos::delay_ms(500);

        led_green.set_low()?;
        led_blue.set_high()?;
        led_red.set_low()?;
        FreeRtos::delay_ms(500);

        led_green.set_low()?;
        led_blue.set_low()?;
        led_red.set_high()?;
        FreeRtos::delay_ms(500);
    }
}
