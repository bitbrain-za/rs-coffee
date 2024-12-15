use crate::sensors::traits::PressureProbe;

pub struct SeeedWaterPressureSensor {
    vcc: f32,
}

impl PressureProbe for SeeedWaterPressureSensor {
    fn convert_voltage_to_pressure(&self, voltage: f32) -> Result<f32, String> {
        Ok((voltage / self.vcc - 0.1) / 0.75)
    }
}
