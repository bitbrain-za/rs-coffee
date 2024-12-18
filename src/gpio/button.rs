use esp_idf_hal::gpio::{InterruptType, PinDriver, Pull};
use esp_idf_svc::hal::gpio::{Input, InputPin, OutputPin};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default, Copy, Clone)]
pub enum ButtonState {
    Pressed,
    #[default]
    Released,
}

impl ButtonState {
    pub fn press(&mut self) {
        *self = ButtonState::Pressed;
    }

    pub fn was_pressed(&mut self) -> bool {
        let res = match self {
            ButtonState::Pressed => true,
            ButtonState::Released => false,
        };

        *self = ButtonState::Released;
        res
    }
}

pub struct Button<'a, PD: InputPin> {
    pin: PinDriver<'a, PD, Input>,
    state: Arc<Mutex<ButtonState>>,
}

impl<'a, PD> Button<'a, PD>
where
    PD: InputPin + OutputPin,
{
    pub fn new(pin: PD, inverted: Option<bool>) -> Self {
        let inverted = inverted.unwrap_or(false);

        let mut button_pin = PinDriver::input(pin).expect("failed to get brew button pin driver");

        let (pull, edge) = if inverted {
            (Pull::Up, InterruptType::NegEdge)
        } else {
            (Pull::Down, InterruptType::PosEdge)
        };

        button_pin
            .set_pull(pull)
            .expect("failed to configure brew button");
        button_pin
            .set_interrupt_type(edge)
            .expect("failed to configure brew button interrupt");

        let state = Arc::new(Mutex::new(ButtonState::Released));
        let state_clone = state.clone();
        unsafe {
            button_pin
                .subscribe(move || {
                    state_clone.lock().unwrap().press();
                })
                .unwrap();
        }

        Self {
            pin: button_pin,
            state,
        }
    }

    pub fn enable(&mut self) {
        self.pin
            .enable_interrupt()
            .expect("failed to enable interrupt");
    }

    pub fn disable(&mut self) {
        self.pin
            .disable_interrupt()
            .expect("failed to disable interrupt");
    }

    pub fn was_pressed(&mut self) -> bool {
        self.enable();
        self.state.lock().unwrap().was_pressed()
    }
}
