pub struct Adc {
    vin_div_top: f32,
}

impl Adc {
    pub fn new(top: u16, vin: f32) -> Self {
        let vin_div_top = vin / top as f32;
        Self { vin_div_top }
    }
    pub fn raw_to_voltage(&self, raw: u16) -> f32 {
        raw as f32 * self.vin_div_top
    }
}
