use esp_idf_hal::{gpio::OutputPin, peripheral::Peripheral, rmt::RmtChannel};
use smart_led_effects::{
    strip::{self, EffectIterator},
    Srgb,
};
use smart_leds_trait::SmartLedsWrite;
use std::sync::mpsc::{channel, Sender};
use std::time::Duration;
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
    Heartbeat,
}

impl State {
    pub fn as_effect(&self, count: usize) -> Box<dyn EffectIterator> {
        match self {
            State::Panic => Box::new(strip::Strobe::new(
                count,
                Some(Srgb::new(255, 0, 0)),
                Duration::from_millis(100),
                None,
            )),
            State::Error => Box::new(strip::Cylon::new(count, Srgb::new(255, 0, 0), None, None)),
            State::Busy => Box::new(strip::RunningLights::new(count, None, false)),
            State::Idle => Box::new(strip::Breathe::new(count, None, None)),
            State::Guage { min, max, level } => {
                let mut progress = strip::ProgressBar::new(
                    count,
                    Some(Srgb::new(0.0, 1.0, 0.0)),
                    Some(Srgb::new(1.0, 0.0, 0.0)),
                    Some(false),
                );

                let percentage: f32 = level / (max - min) * 100.0;
                progress.set_percentage(percentage);

                Box::new(progress)
            }
            State::Temperature { min, max, level } => {
                let mut progress = strip::ProgressBar::new(
                    count,
                    Some(Srgb::new(0.0, 0.0, 1.0)),
                    Some(Srgb::new(1.0, 0.0, 0.0)),
                    Some(true),
                );

                let percentage: f32 = level / (max - min) * 100.0;
                progress.set_percentage(percentage);

                Box::new(progress)
            }
            State::Heartbeat => Box::new(strip::Rainbow::new(1, None)),
            State::Off => Box::new(strip::Breathe::new(count, None, None)),
        }
    }
}

#[derive(Clone)]
pub struct Ring {
    mailbox: Sender<State>,
}

impl Ring {
    pub fn set_state(&self, state: State) {
        self.mailbox.send(state).unwrap();
    }

    pub fn new<C: RmtChannel>(
        rmt_channel: impl Peripheral<P = C> + 'static,
        pin: impl Peripheral<P = impl OutputPin> + 'static,
        tickspeed: std::time::Duration,
        count: usize,
    ) -> Self {
        let mut led = Ws2812Esp32Rmt::new(rmt_channel, pin).expect("Failed to initialize LED ring");
        let (tx, rx) = channel::<State>();

        std::thread::spawn(move || {
            let mut active_state = State::Off;
            let mut effect: Box<dyn EffectIterator> = Box::new(strip::Rainbow::new(count, None));
            log::info!("Starting indicator thread");
            loop {
                while let Ok(state) = rx.try_recv() {
                    if state != active_state {
                        log::debug!("Setting indicator state: {:?}", state);
                        effect = state.as_effect(count);
                        active_state = state;
                    }
                }

                let pixels: Vec<Rgb> = effect
                    .next()
                    .unwrap()
                    .iter()
                    .map(|i| Rgb {
                        r: i.red,
                        g: i.green,
                        b: i.blue,
                    })
                    .collect();
                led.write(pixels).unwrap();
                std::thread::sleep(tickspeed);
            }
        });

        Ring { mailbox: tx }
    }
}
