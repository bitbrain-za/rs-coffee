use super::{postinfusion::PostInfusion, preinfusion::PreInfusion, shot::Shot, Error};
use crate::components::sd_card::SdCard;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::BTreeMap;
use std::fs::{read_dir, File};
use std::io::{Read, Write};

pub type Menu = BTreeMap<u32, String>;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Drink {
    pub name: Option<String>,
    pub preinfusion: Option<PreInfusion>,
    pub shot: Shot,
    pub postinfusion: Option<PostInfusion>,
}

impl Drink {
    // 8.3 filesystem
    const DRINKS_FILE_EXTENSION: &'static str = "JSN";

    pub fn validate(&self) -> Result<(), Error> {
        self.shot.validate()?;
        if let Some(preinfusion) = &self.preinfusion {
            preinfusion.validate()?;
        }

        Ok(())
    }

    pub fn save(&self, menu: &mut Menu) -> anyhow::Result<()> {
        let mut next_file = 0;
        for i in 0..=menu.values().len() as u32 {
            if menu.contains_key(&i) {
                log::info!("File {} exists", i);
            } else {
                next_file = i;
                break;
            }
        }

        log::info!("Next file: {}", next_file);

        let name = self
            .name
            .clone()
            .ok_or(anyhow::anyhow!("Drink name cannot be empty"))?
            .to_lowercase();
        let path = format!(
            "{}/{}.{}",
            SdCard::DRINKS_DIRECTORY,
            next_file,
            Self::DRINKS_FILE_EXTENSION
        );
        let data = serde_json::to_string_pretty(&self)?;
        let mut file = File::create(&path).inspect_err(|e| {
            log::error!("Failed to create file {}: {}", path, e);
        })?;
        log::info!("File {file:?} created");
        file.write_all(data.as_bytes()).inspect_err(|e| {
            log::error!("Failed to write to file {}: {}", path, e);
        })?;

        menu.insert(next_file, name);
        Ok(())
    }

    fn fetch_drink(number: u32) -> anyhow::Result<Self> {
        let path = format!(
            "{}/{}.{}",
            SdCard::DRINKS_DIRECTORY,
            number,
            Self::DRINKS_FILE_EXTENSION
        );
        let mut file = File::open(&path).inspect_err(|e| {
            log::error!("Failed to open file {}: {}", path, e);
        })?;
        let mut file_content = String::new();
        file.read_to_string(&mut file_content).inspect_err(|e| {
            log::error!("Failed to read file {}: {}", path, e);
        })?;
        let drink: Drink = serde_json::from_str(&file_content).inspect_err(|e| {
            log::error!("Failed to parse file {}: {}", path, e);
        })?;
        Ok(drink)
    }

    pub fn load_drink(name: &str, menu: &Menu) -> anyhow::Result<Drink> {
        for (number, drink_name) in menu {
            if drink_name == name {
                return Self::fetch_drink(*number);
            }
        }
        Err(anyhow::anyhow!("Drink not found"))
    }

    pub fn load_all_drinks() -> anyhow::Result<Vec<Drink>> {
        let menu = Self::create_menu()?;
        let mut drinks = Vec::new();
        for (number, name) in &menu {
            log::info!("{}: {}", number, name);
            drinks.push(Self::fetch_drink(*number)?);
        }

        Ok(drinks)
    }

    pub fn create_menu() -> anyhow::Result<Menu> {
        let directory = read_dir(SdCard::DRINKS_DIRECTORY).inspect_err(|e| {
            log::error!("Failed to read directory: {}", e);
        })?;

        let mut menu = Menu::new();

        for entry in directory {
            let entry = entry?;
            log::info!("Entry: {:?}", entry.file_name());
            if let Some(filenumber) = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Invalid file name"))?
                .split(".")
                .try_fold(None, |res, x| {
                    if res.is_none() {
                        Ok(Some(x.parse::<u32>()?))
                    } else if x != Self::DRINKS_FILE_EXTENSION {
                        Err(anyhow::anyhow!("Invalid file extension"))
                    } else {
                        Ok(res)
                    }
                })?
            {
                let drink = Self::fetch_drink(filenumber)?;
                if let Some(name) = drink.name {
                    menu.insert(filenumber, name);
                }
            }
        }

        Ok(menu)
    }
}
