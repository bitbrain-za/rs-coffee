use std::time::Duration;

use esp_idf_hal::{gpio::OutputPin, peripheral::Peripheral, rmt::RmtChannel};
use smart_led_effects::{
    strip::{self, EffectIterator},
    Srgb,
};
use smart_leds_trait::SmartLedsWrite;
use ws2812_esp32_rmt_driver::{Ws2812Esp32Rmt, RGB8 as Rgb};

#[derive(Debug, PartialEq, Clone, Copy, std::default::Default)]
pub enum State {
    #[default]
    Off,
    #[allow(dead_code)]
    Temperature {
        min: f32,
        max: f32,
        level: f32,
    },
    Guage {
        min: f32,
        max: f32,
        level: f32,
    },
    Idle,
    Busy,
    Panic,
    Error,
}

pub struct Ring<'d> {
    pub state: State,
    led: Ws2812Esp32Rmt<'d>,
    effect: Box<dyn EffectIterator>,
    count: usize,
    pub tickspeed: std::time::Duration,
    last_tick: std::time::Instant,
}

impl<'d> Ring<'d> {
    pub fn new<C: RmtChannel>(
        channel: impl Peripheral<P = C> + 'd,
        pin: impl Peripheral<P = impl OutputPin> + 'd,
        count: usize,
        tickspeed: std::time::Duration,
    ) -> Self {
        let led = Ws2812Esp32Rmt::new(channel, pin).expect("Failed to initialize LED ring");
        Self {
            state: State::Off,
            led,
            effect: Box::new(strip::Rainbow::new(count, None)),
            count,
            tickspeed,
            last_tick: std::time::Instant::now() - tickspeed,
        }
    }

    pub fn set_state(&mut self, state: State) {
        match state {
            State::Panic => {
                self.effect = Box::new(strip::Strobe::new(
                    self.count,
                    Some(Srgb::new(255, 0, 0)),
                    Duration::from_millis(100),
                    None,
                ));
            }
            State::Error => {
                self.effect = Box::new(strip::Cylon::new(
                    self.count,
                    Srgb::new(255, 0, 0),
                    None,
                    None,
                ));
            }
            State::Busy => {
                self.effect = Box::new(strip::RunningLights::new(self.count, None, false));
            }
            State::Idle => self.effect = Box::new(strip::Breathe::new(self.count, None, None)),
            State::Guage { min, max, level } => {
                let mut progress = strip::ProgressBar::new(
                    self.count,
                    Some(Srgb::new(0.0, 1.0, 0.0)),
                    Some(Srgb::new(1.0, 0.0, 0.0)),
                    Some(false),
                );

                let percentage: f32 = level / (max - min) * 100.0;
                progress.set_percentage(percentage);

                self.effect = Box::new(progress);
            }
            State::Temperature { min, max, level } => {
                let mut progress = strip::ProgressBar::new(
                    self.count,
                    Some(Srgb::new(0.0, 0.0, 1.0)),
                    Some(Srgb::new(1.0, 0.0, 0.0)),
                    Some(true),
                );

                let percentage: f32 = level / (max - min) * 100.0;
                progress.set_percentage(percentage);

                self.effect = Box::new(progress);
            }
            State::Off => {
                self.effect = Box::new(strip::Breathe::new(self.count, None, None));
            }
        }
        self.state = state;
    }

    pub fn tick(&mut self) -> std::time::Duration {
        let elapsed = self.last_tick.elapsed();
        if elapsed < self.tickspeed {
            return self.tickspeed - elapsed;
        }
        self.last_tick = std::time::Instant::now();
        let pixels: Vec<Rgb> = self
            .effect
            .next()
            .unwrap()
            .iter()
            .map(|i| Rgb {
                r: i.red,
                g: i.green,
                b: i.blue,
            })
            .collect();
        self.led.write(pixels).unwrap();
        let elapsed = self.last_tick.elapsed();
        self.tickspeed - elapsed
    }
}
