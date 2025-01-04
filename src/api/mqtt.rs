use crate::app_state::System;
use crate::config::Mqtt as Config;
use esp_idf_svc::mqtt::client::*;

pub fn mqtt_create(config: Config, system: &System) {
    let system = system.clone();

    let (mut mqtt_client, _mqtt_conn) = EspMqttClient::new(
        &config.url(),
        &MqttClientConfiguration {
            client_id: Some(&config.client_id),
            ..Default::default()
        },
    )
    .unwrap();
    std::thread::spawn(move || loop {
        let events = system.events.lock().unwrap().events.clone();
        system.events.lock().unwrap().events.clear();

        for event in events {
            if event.level > config.event_level {
                continue;
            }
            let _ = mqtt_client.enqueue(
                &config.event_topic,
                QoS::AtMostOnce,
                false,
                event.to_json().as_bytes(),
            );
        }

        let report = system.generate_report().to_json();

        let _ = mqtt_client.enqueue(
            &config.status_topic,
            QoS::AtMostOnce,
            false,
            report.as_bytes(),
        );

        std::thread::sleep(config.report_interval);
    });
}
