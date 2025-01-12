 # 0.0
 - [ ] works same as normal but with temperature control, level sensing and auto-tuning
 - [ ] Review timer implementation (https://docs.esp-rs.org/esp-idf-svc/esp_idf_svc/timer/index.html)
 - [ ] Runtime WIFI config/WPS
 - [ ] Move ADC to I2C ADC (ADS1115)
 - [ ] OTA https://quan.hoabinh.vn/post/2024/03/programming-esp32-with-rust-ota-firmware-update
 - [ ] HA integration
    - [x] discovery
    - [x] state topic
    - [ ] control

 
 # 0.8
 - [ ] Add Availability
 - [ ] Fix up all the state machines/system state/operation state
 - [ ] Maybe an observer patter?
 - [ ] SD Card - Wait for next release: https://github.com/esp-rs/esp-idf-svc/issues/467
 - [ ] Display
 - [ ] Add endpoint to set loadcell scaling
 - [ ] Move DS18b20 to RMT driver on next esp-idf-hal release