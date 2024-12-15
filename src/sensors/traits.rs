pub trait TemperatureProbe {
    fn convert_voltage_to_degrees(&self, voltage: f32) -> Result<f32, String>;
}
