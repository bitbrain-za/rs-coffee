use crate::config::Config;
use esp_idf_svc::nvs::*;
use esp_idf_sys::EspError;
use postcard::{from_bytes, to_vec};

const MAX_VALUE_SIZE: usize = 2056;

#[derive(Debug)]
pub enum Error {
    Timeout,
    EspSys(EspError),
    Serialize(postcard::Error),
    NotFound(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Timeout => write!(f, "Timeout"),
            Error::EspSys(e) => write!(f, "ESP system error: {:?}", e),
            Error::Serialize(e) => write!(f, "Serialization error: {:?}", e),
            Error::NotFound(e) => write!(f, "Not found: {:?}", e),
        }
    }
}

impl std::error::Error for Error {}
pub enum File {
    Config(Config),
}

pub enum FileType {
    Config,
}

impl From<&File> for FileType {
    fn from(file: &File) -> Self {
        match file {
            File::Config(_) => FileType::Config,
        }
    }
}

impl FileType {
    fn key(&self) -> String {
        match self {
            FileType::Config => "config".to_string(),
        }
    }
    pub fn load(&self, fs: &KeyValueStore) -> Result<File, Error> {
        let value_buffer: &mut [u8] = &mut [0; MAX_VALUE_SIZE];

        match self {
            FileType::Config => fs
                .storage
                .get_raw(&self.key(), value_buffer)
                .map_err(Error::EspSys)?
                .map(|val| File::Config(from_bytes::<Config>(val).unwrap_or_default())),
        }
        .ok_or(Error::NotFound(self.key()))
    }
}

impl File {
    fn key(&self) -> String {
        let file_type: FileType = self.into();
        file_type.key()
    }

    pub fn save(&self, fs: &mut KeyValueStore) -> Result<(), Error> {
        let value = match self {
            File::Config(config) => {
                to_vec::<Config, MAX_VALUE_SIZE>(config).map_err(Error::Serialize)?
            }
        };

        fs.storage
            .set_raw(&self.key(), &value)
            .map_err(Error::EspSys)
            .map(|_| ())
    }
}

pub struct KeyValueStore {
    storage: EspNvs<NvsDefault>,
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
}
