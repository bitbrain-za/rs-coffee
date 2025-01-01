use crate::kv_store::{Error as KvsError, Key, KeyValueStore, Storable, Value};
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

impl From<&SeeedWaterPressureSensor> for Value {
    fn from(params: &SeeedWaterPressureSensor) -> Self {
        Value::PressureProbe(*params)
    }
}

impl Storable for SeeedWaterPressureSensor {
    fn load_or_default() -> Self {
        let kvs = match KeyValueStore::new_blocking(std::time::Duration::from_millis(1000)) {
            Ok(kvs) => kvs,
            Err(e) => {
                log::error!("Failed to create key value store: {:?}", e);
                return Self::default();
            }
        };
        match kvs.get(Key::PressureProbe) {
            Value::PressureProbe(calibration) => calibration,
            _ => Self::default(),
        }
    }

    fn save(&self) -> Result<(), KvsError> {
        let mut kvs = KeyValueStore::new_blocking(std::time::Duration::from_millis(1000))?;
        kvs.set(self.into())
    }
}

impl PressureProbe for SeeedWaterPressureSensor {
    fn convert_voltage_to_pressure(&self, voltage: f32) -> Result<f32, String> {
        Ok((voltage / self.vcc - 0.1) / 0.75)
    }
}
