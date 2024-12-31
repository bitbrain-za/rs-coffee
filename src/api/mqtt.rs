use crate::app_state::System;
use crate::config::Mqtt as config;
use crate::schemas::event::LevelFilter;
use esp_idf_svc::mqtt::client::*;

pub fn mqtt_create(url: &str, client_id: &str, system: &System, level: Option<LevelFilter>) {
    let url = url.to_string();
    let client_id = client_id.to_string();
    let system = system.clone();

    let level = level.unwrap_or(LevelFilter::Info);

    let (mut mqtt_client, _mqtt_conn) = EspMqttClient::new(
        &url,
        &MqttClientConfiguration {
            client_id: Some(&client_id),
            ..Default::default()
        },
    )
    .unwrap();
    std::thread::spawn(move || loop {
        let events = system.events.lock().unwrap().events.clone();
        system.events.lock().unwrap().events.clear();

        for event in events {
            if event.level > level {
                continue;
            }
            let _ = mqtt_client.enqueue(
                config::EVENT_TOPIC,
                QoS::AtMostOnce,
                false,
                event.to_json().as_bytes(),
            );
        }

        let report = system.generate_report().to_json();

        let _ = mqtt_client.enqueue(
            config::STATUS_TOPIC,
            QoS::AtMostOnce,
            false,
            report.as_bytes(),
        );

        std::thread::sleep(config::REPORT_INTERVAL);
    });
}
