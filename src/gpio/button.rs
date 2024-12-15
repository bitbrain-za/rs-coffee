#[derive(Default)]
enum ButtonState {
    Pressed,
    #[default]
    Released,
}

#[derive(Default)]
pub struct Button {
    state: ButtonState,
}

impl Button {
    pub fn press(&mut self) {
        self.state = ButtonState::Pressed;
    }

    pub fn was_pressed(&mut self) -> bool {
        let res = match self.state {
            ButtonState::Pressed => true,
            ButtonState::Released => false,
        };

        self.state = ButtonState::Released;
        res
    }
}
