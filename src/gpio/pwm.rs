use esp_idf_svc::hal::gpio::{Output, OutputPin, PinDriver};
use std::time::{Duration, Instant};

pub struct Pwm<'a, PD: OutputPin> {
    out: PinDriver<'a, PD, Output>,
    interval: Duration,
    on_time: Duration,
    start_of_interval: Instant,
    invert: bool,
}

impl<'a, PD> std::fmt::Display for Pwm<'a, PD>
where
    PD: OutputPin,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Pwm: on_time: {:?}, interval: {:?}, invert: {}",
            self.on_time, self.interval, self.invert
        )
    }
}

impl<'a, PD> Pwm<'a, PD>
where
    PD: OutputPin,
{
    pub fn new(pin: PD, interval: Duration, invert: Option<bool>) -> Self {
        Pwm {
            out: PinDriver::output(pin).unwrap(),
            interval,
            on_time: Duration::from_secs(0),
            start_of_interval: Instant::now(),
            invert: invert.unwrap_or(false),
        }
    }

    pub fn set_duty_cycle(&mut self, duty_cycle: f32) {
        let duty_cycle = duty_cycle.clamp(0.0, 1.0);
        self.on_time = self.duty_cycle_to_on_time(duty_cycle, None);
    }

    pub fn get_duty_cycle(&self) -> f32 {
        self.on_time_to_duty_cycle(None, None)
    }

    #[allow(dead_code)]
    pub fn set_interval(&mut self, interval: Duration) {
        let current_dc = self.on_time_to_duty_cycle(None, None);
        self.interval = interval;
        self.on_time = self.duty_cycle_to_on_time(current_dc, None)
    }

    fn on_time_to_duty_cycle(&self, on_time: Option<Duration>, interval: Option<Duration>) -> f32 {
        let on_time = match on_time {
            Some(on_time) => on_time,
            None => self.on_time,
        };
        let interval = match interval {
            Some(interval) => interval,
            None => self.interval,
        };
        on_time.as_secs_f32() / interval.as_secs_f32()
    }

    fn duty_cycle_to_on_time(&self, duty_cycle: f32, interval: Option<Duration>) -> Duration {
        let interval = match interval {
            Some(interval) => interval,
            None => self.interval,
        };
        Duration::from_secs_f32(interval.as_secs_f32() * duty_cycle)
    }

    fn set_on(&mut self) {
        match self.invert {
            true => self.out.set_low(),
            false => self.out.set_high(),
        }
        .expect("Failed to set relay on");
    }

    fn set_off(&mut self) {
        match self.invert {
            true => self.out.set_high(),
            false => self.out.set_low(),
        }
        .expect("Failed to set relay off");
    }

    pub fn tick(&mut self) -> Option<Duration> {
        if self.on_time == Duration::from_secs(0) {
            self.set_off();
            return None;
        }

        if self.on_time == self.interval {
            self.set_on();
            return None;
        }

        let mut time_in_cycle = self.start_of_interval.elapsed();
        if time_in_cycle > self.interval {
            self.start_of_interval = Instant::now();
            time_in_cycle = Duration::from_secs(0);
        }

        let time_to_state_change = if time_in_cycle < self.on_time {
            self.set_on();
            self.on_time
        } else {
            self.set_off();
            self.interval - self.on_time
        };

        Some(time_to_state_change)
    }
}

pub struct PwmBuilder<'a, PD: OutputPin> {
    pin: Option<PD>,
    interval: Option<Duration>,
    invert: Option<bool>,
    _phantom: std::marker::PhantomData<&'a PD>,
}

impl<'a, PD> PwmBuilder<'a, PD>
where
    PD: OutputPin,
{
    pub fn new() -> Self {
        PwmBuilder {
            pin: None,
            interval: None,
            invert: None,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn with_pin(mut self, pin: PD) -> Self {
        self.pin = Some(pin);
        self
    }

    #[allow(dead_code)]
    pub fn invert(mut self, invert: bool) -> Self {
        self.invert = Some(invert);
        self
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = Some(interval);
        self
    }

    pub fn build(self) -> Pwm<'a, PD> {
        let pin = self.pin.expect("Pin is required");
        let interval = self.interval.expect("Interval is required");
        Pwm::new(pin, interval, self.invert)
    }
}
