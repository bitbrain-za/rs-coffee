use crate::app_state::System;
use crate::schemas::drink::Drink;
use anyhow::Result;

pub fn put_drink(data: &str, system: System) -> Result<()> {
    let drink: Drink = serde_json::from_str(data)?;
    drink.validate()?;
    *system.drink.write().unwrap() = Some(drink);
    Ok(())
}

pub fn post_drink(_data: &str, _system: System) -> Result<()> {
    todo!();
}
