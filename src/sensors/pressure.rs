use crate::sensors::traits::PressureProbe;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
pub struct SeeedWaterPressureSensor {
    vcc: f32,
}

impl Default for SeeedWaterPressureSensor {
    fn default() -> Self {
        SeeedWaterPressureSensor { vcc: 3.3 }
    }
}

impl PressureProbe for SeeedWaterPressureSensor {
    fn convert_voltage_to_pressure(&self, voltage: f64) -> Result<f32, String> {
        Ok(((voltage / self.vcc as f64 - 0.1) / 0.75) as f32)
    }
}
