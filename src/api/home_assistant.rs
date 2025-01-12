pub struct HomeAssistantIntegration {}

impl HomeAssistantIntegration {
    pub fn discovery_message(id: &str) -> (String, String) {
        use dotenv_codegen::dotenv;
        let model = dotenv!("MODEL");
        let name_lc = dotenv!("NAME");
        let name = dotenv!("NAME");
        let hardware = dotenv!("HW");
        let serial = dotenv!("SERIAL");
        let version = env!("CARGO_PKG_VERSION");

        let topic = format!(
            "homeassistant/device/{}/{}/config",
            name_lc.to_lowercase(),
            id
        );

        let message = serde_json::json!({
            "dev": {
                "ids": format!("{}-{}", name, id),
                "name": "Espresso Machine",
                "mf": "bitbrain",
                "mdl": format!("{} {}", name, model),
                "sw": version,
                "sn": serial,
                "hw": hardware,
                "suggested_area": "Kitchen"
            },
            "o": {
                "name": name,
                "sw": version,
                "url": "https://github.com/bitbrain-za/rs-coffee"
            },
            "cmps": {
                "on_button": {
                    "p": "switch",
                    "device_class": "switch",
                    "name": "Power Switch",
                    "unique_id": "power_switch",
                    "command_topic": format!("{}/{}/set/power", name_lc, id),
                    "payload_off": "off",
                    "payload_on": "on",
                    "value_template": "{{ 'OFF' if value_json.status == \"Off\" else 'ON'}}"
                },
                "boiler": {
                    "p": "sensor",
                    "device_class": "temperature",
                    "unit_of_measurement": "°C",
                    "value_template": "{{ value_json.device.temperature}}",
                    "name": "Boiler Temperature",
                    "unique_id": "temperature_boiler"
                },
                "ambient_temperature": {
                    "p": "sensor",
                    "device_class": "temperature",
                    "unit_of_measurement": "°C",
                    "value_template": "{{ value_json.device.ambient}}",
                    "unique_id": "temperature_ambient",
                    "name": "Ambient Temperature"
                },
                "pump": {
                    "name": "Pressure",
                    "p": "sensor",
                    "device_class": "pressure",
                    "unit_of_measurement": "bar",
                    "value_template": "{{ value_json.device.pressure}}",
                    "unique_id": "pressure_pump"
                },
                "reservoir_level": {
                    "name": "Water Level",
                    "icon": "mdi:water-circle",
                    "p": "sensor",
                    "device_class": "volume_storage",
                    "unit_of_measurement": "mL",
                    "value_template": "{{ value_json.device.level}}",
                    "unique_id": "level_reservoir"
                },
                "power": {
                    "name": "Power",
                    "p": "sensor",
                    "device_class": "power",
                    "unit_of_measurement": "W",
                    "value_template": "{{ value_json.device.power}}",
                    "unique_id": "level_reservoir"
                },
                "weight": {
                    "p": "sensor",
                    "device_class": "weight",
                    "unit_of_measurement": "g",
                    "value_template": "{{ value_json.device.weight}}",
                    "unique_id": "level_reservoir"
                },
                "switch_brew": {
                    "name": "Brew Switch",
                    "icon": "mdi:coffee-maker-outline",
                    "p": "binary_sensor",
                    "value_template": "{{ value_json.device.switches.brew}}",
                    "unique_id": "switch_brew"
                },
                "switch_water": {
                    "name": "Water Switch",
                    "icon": "mdi:water",
                    "p": "binary_sensor",
                    "value_template": "{{ value_json.device.switches.water}}",
                    "unique_id": "switch_water"
                },
                "switch_steam": {
                    "name": "Steam Switch",
                    "icon": "mdi:kettle-steam",
                    "p": "binary_sensor",
                    "value_template": "{{ value_json.device.switches.steam}}",
                    "unique_id": "switch_pump"
                },
                "boiler_target": {
                    "p": "number",
                    "device_class": "temperature",
                    "unit_of_measurement": "°C",
                    "value_template": "{{ value_json.device.temperature}}",
                    "unique_id": "boiler_target",
                    "command_topic": format!("{}/{}/set/boiler_temperature", name_lc, id),
                    "max": 140,
                    "min": 0,
                    "step": 0.1
                },
                "pump_target": {
                    "p": "number",
                    "device_class": "pressure",
                    "unit_of_measurement": "bar",
                    "value_template": "{{ value_json.device.pressure}}",
                    "unique_id": "pump_target",
                    "command_topic": format!("{}/{}/set/pump_pressure", name_lc, id),
                    "max": 12,
                    "min": 0,
                    "step": 0.5
                }
            },
            "state_topic": format!("{}/{}/state", name_lc, id),
            "qos": 2,
        })
        .to_string();

        (topic, message)
    }
}
