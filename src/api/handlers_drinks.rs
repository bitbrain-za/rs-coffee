use crate::app_state::System;
#[cfg(feature = "sdcard")]
use crate::schemas::drink::Drink;
use anyhow::Result;

#[cfg(feature = "sdcard")]
#[derive(Debug)]
pub enum Error {
    System(String),
}

#[cfg(feature = "sdcard")]
impl std::error::Error for Error {}
#[cfg(feature = "sdcard")]
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::System(s) => write!(f, "System Error: {}", s),
        }
    }
}

#[cfg(feature = "sdcard")]
pub fn put_drink(data: &str, system: System) -> Result<()> {
    if !*system.sd_card_present {
        Err(Error::System("No SD card present".to_string()).into())
    } else {
        let drink: Drink = serde_json::from_str(data)?;
        drink.validate()?;

        let mut menu = system
            .menu
            .write()
            .map_err(|_| Error::System("Failed to write menu".to_string()))?;
        drink.save(&mut menu)
    }
}

#[cfg(feature = "sdcard")]
pub fn get_drink(data: &str, system: System) -> Result<serde_json::Value> {
    if !*system.sd_card_present {
        Err(Error::System("No SD card present".to_string()).into())
    } else {
        let parts = data.split('?').collect::<Vec<&str>>();
        let drinks = if parts.len() > 1 {
            let mut drinks = Vec::new();
            for part in parts[1].split('&') {
                let parts = part.split('=').collect::<Vec<&str>>();
                if parts.len() == 2 {
                    let key = parts[0];
                    let value = parts[1];
                    if key == "name" {
                        let menu = system
                            .menu
                            .read()
                            .map_err(|_| Error::System("Failed to read menu".to_string()))?;
                        drinks.push(Drink::load_drink(value, &menu)?);
                    }
                }
            }
            drinks
        } else {
            log::info!("Loading all drinks");
            Drink::load_all_drinks()?
        };
        Ok(serde_json::to_value(drinks)?)
    }
}

pub fn post_drink(_data: &str, _system: System) -> Result<()> {
    todo!();
}
