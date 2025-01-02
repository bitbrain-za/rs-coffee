use crate::types::Millimeters;
use esp_idf_hal::delay::NON_BLOCK;
use esp_idf_hal::{
    gpio::{self, InputPin, OutputPin},
    peripheral::Peripheral,
    prelude::*,
    uart::*,
};
use std::sync::{
    mpsc::{channel, Sender},
    Arc, RwLock,
};

#[derive(Clone)]
pub struct A02yyuw {
    pub distance: Arc<RwLock<Millimeters>>,
    mailbox: Sender<Message>,
}

#[allow(dead_code)]
pub enum Message {
    DoRead,
}

impl A02yyuw {
    pub fn send_message(&self, message: Message) {
        self.mailbox.send(message).unwrap();
    }
    pub fn new<UART: Uart>(
        uart: impl Peripheral<P = UART> + 'static,
        rx: impl Peripheral<P = impl InputPin> + 'static,
        tx: impl Peripheral<P = impl OutputPin> + 'static,
    ) -> Self {
        log::info!("Starting UART");
        let config = config::Config::new().baudrate(Hertz(9600));
        let uart = UartDriver::new(
            uart,
            tx,
            rx,
            Option::<gpio::Gpio0>::None,
            Option::<gpio::Gpio1>::None,
            &config,
        )
        .expect("Failed to initialize UART");

        let (tx, rx) = channel::<Message>();
        let distance = Arc::new(RwLock::new(Millimeters::default()));
        let distance_clone = distance.clone();
        let polling_interval = std::time::Duration::from_secs(30);
        log::info!("Starting A02YYUW thread");
        std::thread::spawn(move || loop {
            // For now we really don't care why we returned, there's only one command
            let _ = rx.recv_timeout(polling_interval);

            let mut buffer1 = [0; 1];
            let mut buffer2 = [0; 2];

            log::info!("Reading buffer");
            let start = std::time::Instant::now();
            if loop {
                if let Ok(1) = uart.read(&mut buffer1, NON_BLOCK) {
                    if buffer1[0] != 0xFF {
                        if let Ok(2) = uart.read(&mut buffer2, NON_BLOCK) {
                            break true;
                        }
                    }
                }
                if start.elapsed() > std::time::Duration::from_secs(3) {
                    log::warn!("Timeout reading buffer");
                    *distance_clone.write().unwrap() = 0;
                    break false;
                }
            } {
                let expected = buffer1[0].wrapping_add(buffer2[0]).wrapping_add(0xFF);
                if expected != buffer2[1] {
                    log::warn!("Checksum mismatch: {:02X} != {:02X}", expected, buffer2[1]);
                    continue;
                }
                *distance_clone.write().unwrap() =
                    (buffer1[0] as Millimeters) << 8 | (buffer2[0] as Millimeters);
            }
        });

        A02yyuw {
            distance,
            mailbox: tx,
        }
    }
}
