use crate::api::home_assistant::HomeAssistantIntegration;
use crate::app_state::System;
use crate::config::Mqtt as Config;
use esp_idf_svc::mqtt::client::*;

#[derive(Debug)]
pub enum Command {
    PowerOn,
    PowerOff,
    SetTemperature(f32),
    SetPressure(f32),
}

impl<E> TryFrom<&EventPayload<'_, E>> for Command
where
    E: std::fmt::Debug,
{
    type Error = &'static str;
    fn try_from(event: &EventPayload<E>) -> Result<Self, Self::Error> {
        if let EventPayload::Received {
            id: _,
            topic,
            data,
            details: _,
        } = event
        {
            let command = topic
                .ok_or("No topic :(")?
                .split('/')
                .last()
                .ok_or("Invalid topic")?;
            let payload: String = data.iter().map(|b| *b as char).collect();
            log::debug!("Command: {} Payload: {}", command, payload);
            match command {
                "power" => match payload.to_lowercase().as_str() {
                    "on" => Ok(Command::PowerOn),
                    "off" => Ok(Command::PowerOff),
                    _ => Err("Invalid power command"),
                },
                "temperature" => Ok(Command::SetTemperature(
                    payload.parse().map_err(|_| "Invalid temperature")?,
                )),
                "pressure" => Ok(Command::SetPressure(
                    payload.parse().map_err(|_| "Invalid pressure")?,
                )),
                _ => Err("Invalid command"),
            }
        } else {
            Err("Invalid event")
        }
    }
}

impl Command {
    fn execute(&self, system: &System) {
        log::info!("Executing command: {:?}", self);
        match self {
            Command::PowerOn => system.set_temperature(60.0),
            Command::PowerOff => {
                system.set_temperature(0.0);
                system.set_pressure(0.0);
            }
            Command::SetTemperature(temperature) => system.set_temperature(*temperature),
            Command::SetPressure(pressure) => system.set_pressure(*pressure),
        }
    }
}

pub fn mqtt_create(config: Config, system: &System) {
    let system = system.clone();
    let event_topic = config
        .event_topic
        .clone()
        .replace("<ID>", &system.board.mac);
    let status_topic = config
        .status_topic
        .clone()
        .replace("<ID>", &system.board.mac);

    log::info!("Event topic: {}", event_topic);
    log::info!("Status topic: {}", status_topic);

    let (mut mqtt_client, mut mqtt_conn) = EspMqttClient::new(
        &config.url(),
        &MqttClientConfiguration {
            client_id: Some(&config.client_id),
            ..Default::default()
        },
    )
    .unwrap();

    let system_for_subscriber = system.clone();
    std::thread::Builder::new()
        .stack_size(6 * 1024)
        .name("MQTT_sub".to_string())
        .spawn(move || {
            while let Ok(event) = mqtt_conn.next() {
                mqtt_event_handler(event, system_for_subscriber.clone());
            }
            log::info!("Connection closed");
        })
        .unwrap();

    std::thread::Builder::new()
        .stack_size(6 * 1024)
        .spawn(move || {
            let (discovery_topic, discovery_message) =
                HomeAssistantIntegration::discovery_message(&system.board.mac);
            let _ = mqtt_client.enqueue(
                &discovery_topic,
                QoS::AtMostOnce,
                true,
                discovery_message.as_bytes(),
            );

            let topic = format!(
                "{}/{}/set/#",
                dotenv_codegen::dotenv!("NAME").to_lowercase(),
                &system.board.mac
            );
            loop {
                if let Err(e) = mqtt_client.subscribe(&topic, QoS::AtMostOnce) {
                    log::error!("Failed to subscribe to topic: {}", e);
                    std::thread::sleep(std::time::Duration::from_secs(2));
                } else {
                    log::info!("Subscribed to topic: {}", topic);
                    break;
                }
            }

            loop {
                let events = system.events.lock().unwrap().events.clone();
                system.events.lock().unwrap().events.clear();

                for event in events {
                    if event.level > config.event_level {
                        continue;
                    }
                    let _ = mqtt_client.enqueue(
                        &event_topic,
                        QoS::AtMostOnce,
                        false,
                        event.to_json().as_bytes(),
                    );
                }

                let report = system.generate_report().to_json();

                let _ =
                    mqtt_client.enqueue(&status_topic, QoS::AtMostOnce, false, report.as_bytes());

                std::thread::sleep(config.report_interval);
            }
        })
        .expect("Failed to start MQTT thread");
}

fn mqtt_event_handler(event: &EspMqttEvent, system: System) {
    log::debug!("Event: {:?}", event.payload());

    let payload = event.payload();
    match Command::try_from(&payload) {
        Ok(command) => command.execute(&system),
        Err(e) => {
            log::error!("Failed to parse command: {}", e);
        }
    };
}
