use esp_idf_svc::hal::gpio::{Output, OutputPin, PinDriver};
use std::time::{self, Duration, Instant};

pub struct Pwm<'a, PD: OutputPin> {
    out: PinDriver<'a, PD, Output>,
    interval: Duration,
    on_time: Duration,
    start_of_interval: Instant,
    invert: bool,
    poll_rate: Duration,
    last_poll: Instant,
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
    pub fn new(pin: PD, interval: Duration, poll_rate: Duration, invert: Option<bool>) -> Self {
        Pwm {
            out: PinDriver::output(pin).unwrap(),
            interval,
            on_time: Duration::from_secs(0),
            start_of_interval: Instant::now(),
            invert: invert.unwrap_or(false),
            last_poll: Instant::now() - poll_rate,
            poll_rate,
        }
    }

    pub fn set_duty_cycle(&mut self, duty_cycle: f32) {
        let duty_cycle = duty_cycle.clamp(0.0, 1.0);
        self.on_time = self.duty_cycle_to_on_time(duty_cycle, None);
    }

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

    pub fn tick(&mut self) -> Duration {
        let time_since_last_poll = self.last_poll.elapsed();
        if time_since_last_poll < self.poll_rate {
            return self.poll_rate - time_since_last_poll;
        }

        let mut time_in_cycle = self.start_of_interval.elapsed();
        if time_in_cycle > self.interval {
            self.start_of_interval = Instant::now();
            time_in_cycle = Duration::from_secs(0);
        }

        let _ = if time_in_cycle < self.on_time {
            match self.invert {
                true => self.out.set_low(),
                false => self.out.set_high(),
            }
        } else {
            match self.invert {
                true => self.out.set_high(),
                false => self.out.set_low(),
            }
        };

        self.last_poll = Instant::now();
        self.poll_rate
    }
}

pub struct PwmBuilder<'a, PD: OutputPin> {
    pin: Option<PD>,
    interval: Option<Duration>,
    poll_rate: Option<Duration>,
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
            poll_rate: None,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn with_pin(mut self, pin: PD) -> Self {
        self.pin = Some(pin);
        self
    }

    pub fn invert(mut self, invert: bool) -> Self {
        self.invert = Some(invert);
        self
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = Some(interval);
        self
    }

    pub fn with_poll_rate(mut self, poll_rate: Duration) -> Self {
        self.poll_rate = Some(poll_rate);
        self
    }

    pub fn build(self) -> Pwm<'a, PD> {
        let pin = self.pin.expect("Pin is required");
        let interval = self.interval.expect("Interval is required");
        let poll_rate = self.poll_rate.expect("Poll rate is required");
        Pwm::new(pin, interval, poll_rate, self.invert)
    }
}
