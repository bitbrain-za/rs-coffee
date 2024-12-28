use crate::app_state::ApiState;
use crate::schemas::drink::Drink;
use anyhow::Result;

pub fn put_drink(data: &str, system: ApiState) -> Result<()> {
    let drink: Drink = serde_json::from_str(data)?;
    drink.validate()?;
    system.lock().unwrap().drink = Some(drink);
    Ok(())
}

pub fn post_drink(data: &str, system: ApiState) -> Result<()> {
    system.lock().unwrap().echo_data = data.to_string();
    Ok(())
}
