use core::time::Duration;
use esp_idf_svc::mqtt::client::*;

pub fn mqtt_create(url: &str, client_id: &str) {
    let url = url.to_string();
    let client_id = client_id.to_string();

    let (mut mqtt_client, _mqtt_conn) = EspMqttClient::new(
        &url,
        &MqttClientConfiguration {
            client_id: Some(&client_id),
            ..Default::default()
        },
    )
    .unwrap();

    std::thread::spawn(move || loop {
        let payload = "dummy";
        let _ = mqtt_client.enqueue("test", QoS::AtMostOnce, false, payload.as_bytes());

        std::thread::sleep(Duration::from_secs(2));
    });
}
