pub trait TemperatureProbe {
    fn convert_voltage_to_degrees(&self, voltage: f64) -> Result<f32, String>;
}
pub trait PressureProbe {
    fn convert_voltage_to_pressure(&self, voltage: f64) -> Result<f32, String>;
}
