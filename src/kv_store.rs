use crate::models::boiler::BoilerModelParameters;
use crate::sensors::{pressure::SeeedWaterPressureSensor, pt100::Pt100, scale::ScaleConfig};
use esp_idf_svc::nvs::*;
use esp_idf_sys::EspError;
use postcard::{from_bytes, to_vec};

#[derive(Debug)]
pub enum Error {
    Timeout,
    EspSys(EspError),
    Serialize(postcard::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Timeout => write!(f, "Timeout"),
            Error::EspSys(e) => write!(f, "ESP system error: {:?}", e),
            Error::Serialize(e) => write!(f, "Serialization error: {:?}", e),
        }
    }
}

pub trait Storable {
    fn load_or_default() -> Self;
    fn save(&self) -> Result<(), Error>;
}

pub enum Key {
    BoilerModelParameters,
    ScaleParameters,
    TemperatureProbe,
    PressureProbe,
}

use serde::Serialize;

#[derive(Serialize)]
pub enum Value {
    BoilerModelParameters(BoilerModelParameters),
    ScaleParameters(ScaleConfig),
    TemperatureProbe(Pt100),
    PressureProbe(SeeedWaterPressureSensor),
}

impl From<&Value> for Key {
    fn from(value: &Value) -> Self {
        match value {
            Value::BoilerModelParameters(_) => Key::BoilerModelParameters,
            Value::ScaleParameters(_) => Key::ScaleParameters,
            Value::TemperatureProbe(_) => Key::TemperatureProbe,
            Value::PressureProbe(_) => Key::PressureProbe,
        }
    }
}

impl From<Key> for Value {
    fn from(key: Key) -> Self {
        match key {
            Key::BoilerModelParameters => {
                Value::BoilerModelParameters(BoilerModelParameters::default())
            }
            Key::ScaleParameters => Value::ScaleParameters(ScaleConfig::default()),
            Key::TemperatureProbe => Value::TemperatureProbe(Pt100::default()),
            Key::PressureProbe => Value::PressureProbe(SeeedWaterPressureSensor::default()),
        }
    }
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Key::BoilerModelParameters => write!(f, "p_boiler"),
            Key::ScaleParameters => write!(f, "p_scale"),
            Key::TemperatureProbe => write!(f, "p_temperature"),
            Key::PressureProbe => write!(f, "p_pressure"),
        }
    }
}

const MAX_VALUE_SIZE: usize = 1024;

pub struct KeyValueStore {
    storage: EspNvs<NvsDefault>,
}

impl std::fmt::Debug for KeyValueStore {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

impl KeyValueStore {
    pub fn new() -> Result<Self, String> {
        let nvs_default_partition: EspNvsPartition<NvsDefault> = EspDefaultNvsPartition::take()
            .map_err(|e| format!("Couldn't get default partition: {:?}", e))?;

        let namespace = "rs-coffee";
        let nvs = EspNvs::new(nvs_default_partition, namespace, true).map_err(|e| {
            format!(
                "Couldn't get namespace {:?} in default partition: {:?}",
                namespace, e
            )
        })?;
        Ok(Self { storage: nvs })
    }

    pub fn new_blocking(timeout: std::time::Duration) -> Result<Self, Error> {
        let expires = std::time::Instant::now() + timeout;
        loop {
            match Self::new() {
                Ok(store) => return Ok(store),
                Err(_) => {
                    if std::time::Instant::now() > expires {
                        return Err(Error::Timeout);
                    }
                }
            }
        }
    }

    pub fn get(&self, key: Key) -> Value {
        let value_buffer: &mut [u8] = &mut [0; MAX_VALUE_SIZE];

        match self.storage.get_raw(&key.to_string(), value_buffer) {
            Err(e) => {
                log::info!("Couldn't get {} from nvs: {:?}", key, e);
                key.into()
            }
            Ok(res) => match res {
                Some(val) => match key {
                    Key::BoilerModelParameters => Value::BoilerModelParameters(
                        from_bytes::<BoilerModelParameters>(val).unwrap_or_default(),
                    ),
                    Key::ScaleParameters => {
                        Value::ScaleParameters(from_bytes::<ScaleConfig>(val).unwrap_or_default())
                    }
                    Key::TemperatureProbe => {
                        Value::TemperatureProbe(from_bytes::<Pt100>(val).unwrap_or_default())
                    }
                    Key::PressureProbe => Value::PressureProbe(
                        from_bytes::<SeeedWaterPressureSensor>(val).unwrap_or_default(),
                    ),
                },
                None => {
                    log::info!("No value found for key {}. Setting to default", key);
                    key.into()
                }
            },
        }
    }

    pub fn set(&mut self, value: Value) -> Result<(), Error> {
        let key = Key::from(&value).to_string();
        let value = &to_vec::<Value, MAX_VALUE_SIZE>(&value).map_err(Error::Serialize)?;
        self.storage
            .set_raw(&key, value)
            .map_err(Error::EspSys)
            .map(|_| ())
    }
}
