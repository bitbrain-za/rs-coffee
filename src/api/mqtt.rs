use crate::app_state::System;
use crate::config::Mqtt as config;
use crate::schemas::status::StatusReport;
use core::time::Duration;
use esp_idf_svc::mqtt::client::*;

pub fn mqtt_create(url: &str, client_id: &str, system: &System) {
    let url = url.to_string();
    let client_id = client_id.to_string();
    let system = system.clone();

    let (mut mqtt_client, _mqtt_conn) = EspMqttClient::new(
        &url,
        &MqttClientConfiguration {
            client_id: Some(&client_id),
            ..Default::default()
        },
    )
    .unwrap();
    std::thread::spawn(move || loop {
        let status = StatusReport::new(&system);
        let payload = status.to_json();

        let _ = mqtt_client.enqueue(
            config::STATUS_TOPIC,
            QoS::AtMostOnce,
            false,
            payload.as_bytes(),
        );

        std::thread::sleep(Duration::from_secs(2));
    });
}
