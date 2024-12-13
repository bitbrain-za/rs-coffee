use crate::mock_boiler::MockBoiler;
use core::borrow::Borrow;
use esp_idf_svc::hal::adc::oneshot::config::AdcChannelConfig;
use esp_idf_svc::hal::{
    adc::oneshot::{AdcChannelDriver, AdcDriver},
    gpio::{ADCPin, Output, OutputPin, PinDriver},
};
use esp_idf_svc::sys::EspError as Error;
use pid_ctrl::{self, PidCtrl, PidIn};
use std::env;

pub struct BoilerPid<'a, T: ADCPin, M: Borrow<AdcDriver<'a, T::Adc>>, O: OutputPin> {
    adc: AdcChannelDriver<'a, T, M>,
    out: PinDriver<'a, O, Output>,
    pid: PidCtrl<f32>,
    last_reading: std::time::Instant,
    window_size: std::time::Duration,
    window_start: std::time::Instant,
    update_interval: std::time::Duration,
    mock_boiler: MockBoiler,
}

impl<'a, T, M, O> BoilerPid<'a, T, M, O>
where
    T: ADCPin,
    M: Borrow<AdcDriver<'a, T::Adc>>,
    O: OutputPin,
{
    pub fn new(adc: M, adc_pin: T, output_pin: O, target: f32) -> Result<Self, Error> {
        let probe_config = AdcChannelConfig::new();
        let kp = env::var("KP")
            .unwrap_or("2.0".to_string())
            .parse::<f32>()
            .unwrap_or(2.0);
        let ki = env::var("KI")
            .unwrap_or("0.01".to_string())
            .parse::<f32>()
            .unwrap_or(1.0);
        let kd = env::var("KD")
            .unwrap_or("2.0".to_string())
            .parse::<f32>()
            .unwrap_or(1.0);

        log::info!(
            "PID Config: KP: {}, KI: {}, KD: {}, Target: {}",
            kp,
            ki,
            kd,
            target
        );

        let mut pid = pid_ctrl::PidCtrl::new_with_pid(kp, ki, kd);
        pid.limits.try_set_lower(0.0).unwrap();
        pid.limits.try_set_upper(5.0).unwrap();
        pid.ki.limits.try_set_upper(5.0).unwrap();
        pid.ki.limits.try_set_lower(0.0).unwrap();

        let mut adc = AdcChannelDriver::new(adc, adc_pin, &probe_config).unwrap();
        let initial_reading = adc.read()? as f32;

        pid.init(target, initial_reading);

        Ok(Self {
            adc,
            out: PinDriver::output(output_pin)?,
            pid,
            last_reading: std::time::Instant::now(),
            window_size: std::time::Duration::from_millis(5000),
            window_start: std::time::Instant::now(),
            update_interval: std::time::Duration::from_millis(1000),
            mock_boiler: MockBoiler::new(20.0),
        })
    }

    pub fn set_target(&mut self, target: f32) {
        self.pid.setpoint = target;
    }

    pub fn poll(&mut self) -> Result<(), Error> {
        self.mock_boiler.tick();
        if std::time::Instant::now() - self.last_reading < self.update_interval {
            Ok(())
        } else {
            // let temp = self.adc.read()? as f32;
            // let temp = Self::convert_raw_to_celsius(temp);
            let temp = self.mock_boiler.get_temperature();

            let delta_t = self.last_reading.elapsed().as_secs_f32();
            self.last_reading = std::time::Instant::now();

            let out = self.pid.step(PidIn::new(temp, delta_t));
            log::info!("{:?}", out);

            if std::time::Instant::now() - self.window_start > self.window_size {
                self.window_start = std::time::Instant::now();
            }
            let point_in_window = self.window_start.elapsed().as_secs_f32();
            self.mock_boiler.set_control(out.out / 5.0);

            if out.out > point_in_window {
                self.turn_on()?;
            } else {
                self.turn_off()?;
            }

            Ok(())
        }
    }

    pub fn turn_on(&mut self) -> Result<(), Error> {
        self.out.set_high()
    }

    pub fn turn_off(&mut self) -> Result<(), Error> {
        self.out.set_low()
    }

    fn convert_raw_to_celsius(raw: f32) -> f32 {
        const VCC: f32 = 3.30;
        const BETA: f32 = 3977.0;
        const T0: f32 = 25.0 + 273.35;
        const R0: f32 = 10_000.0;

        let voltage = (VCC / 1024.0) * raw;
        let voltage_delta = VCC - voltage;
        let resistance = voltage / (voltage_delta / R0);
        let ln = (resistance / R0).ln();
        let temperature = 1.0 / ((ln / BETA) + 1.0 / T0);
        temperature - 273.35
        // temperature

        // Conversion logic goes here
        // 1.0 / ((1.0 / (1024.0 / raw - 1.0)).ln() / BETA + 1.0 / 298.35) - 273.35
    }
}
