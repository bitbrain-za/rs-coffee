use crate::api::home_assistant::HomeAssistantIntegration;
use crate::app_state::System;
use crate::config::Mqtt as Config;
use esp_idf_svc::mqtt::client::*;

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

    std::thread::Builder::new()
        .name("MQTT_sub".to_string())
        .spawn(move || {
            while let Ok(event) = mqtt_conn.next() {
                log::info!("[Queue] Event: {}", event.payload());
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
