use anyhow::Result;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::prelude::Peripherals;
use std::sync::{Arc, Mutex};
mod app_state;
mod board;
mod indicator;

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

    let app_state = app_state::AppState::new();
    let app_state = Arc::new(Mutex::new(app_state));

    let app_state_indicator = app_state.clone();

    std::thread::spawn(move || {
        let mut ring =
            indicator::ring::Ring::new(channel, led_pin, 32, std::time::Duration::from_millis(100));
        ring.set_state(indicator::ring::State::Busy);

        loop {
            if ring.state != app_state_indicator.lock().unwrap().indicator_state {
                ring.set_state(app_state_indicator.lock().unwrap().indicator_state);
            }
            let next_tick = ring.tick();
            FreeRtos::delay_ms(next_tick.as_millis() as u32);
        }
    });

    app_state.lock().unwrap().indicator_state = indicator::ring::State::Busy;

    FreeRtos::delay_ms(5000);

    let mut level = 0.0;
    let mut start = std::time::Instant::now() - std::time::Duration::from_millis(200);
    loop {
        if start.elapsed() > std::time::Duration::from_millis(200) {
            app_state.lock().unwrap().indicator_state = indicator::ring::State::Guage {
                min: 0.0,
                max: 100.0,
                level,
            };

            level += 1.0;
            if level > 100.0 {
                level = 0.0;
            }
            start = std::time::Instant::now();
        }

        FreeRtos::delay_ms(100);
    }
}
