use crate::app_state::System;
use crate::board::{Action, F32Read};
use crate::config;
use crate::models::boiler::{BoilerModel, BoilerModelParameters};
use std::time::{Duration, Instant};

#[derive(Debug, Copy, Clone)]
pub struct DataPoint {
    time: Instant,
    power: f32,
    probe_temperature: f32,
}

pub enum State {
    Setup,
    GatheringData,
    AnalyzingData,
    Done,
}

impl Iterator for State {
    type Item = State;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            State::Setup => {
                *self = State::GatheringData;
                Some(State::GatheringData)
            }
            State::GatheringData => {
                *self = State::AnalyzingData;
                Some(State::AnalyzingData)
            }
            State::AnalyzingData => {
                *self = State::Done;
                Some(State::Done)
            }
            State::Done => None,
        }
    }
}

pub struct AutoTuner<'a> {
    state: State,
    setpoint: f32,
    boiler_model: BoilerModel,
    parameters: Option<BoilerModelParameters>,
    start_time: Instant,
    duration: Duration,
    delta_time: Duration,
    data_points: Vec<DataPoint>,
    next_reading: Instant,
    system: System<'a>,
}

impl<'a> AutoTuner<'a> {
    pub fn new(setpoint: f32, system: System<'a>) -> Self {
        Self {
            state: State::Setup,
            setpoint,
            boiler_model: BoilerModel::new(None),
            parameters: None,
            start_time: Instant::now(),
            duration: Duration::from_secs(60),
            delta_time: Duration::from_secs(1),
            next_reading: Instant::now(),
            data_points: Vec::new(),
            system,
        }
    }

    pub fn run(&mut self) -> Option<BoilerModelParameters> {
        match self.state {
            State::Setup => {
                println!("Setting up auto-tuner");
                self.state = State::GatheringData;
                None
            }
            State::GatheringData => {
                if self.next_reading > Instant::now() {
                    return None;
                }

                let power =
                    self.system.read_f32(F32Read::BoilerDutyCycle) * config::BOILER_POWER as f32;
                let boiler_temperature = self.system.read_f32(F32Read::BoilerTemperature);
                self.data_points.push(DataPoint {
                    time: Instant::now(),
                    power,
                    probe_temperature: boiler_temperature,
                });

                let mut dc = if boiler_temperature > self.setpoint {
                    1.0
                } else {
                    0.0
                };

                if self.start_time.elapsed() >= self.duration {
                    dc = 0.0;
                    self.state = State::AnalyzingData;
                }

                self.system
                    .execute_board_action(Action::SetBoialerDutyCycle(dc))
                    .expect("Failed to set boiler duty cycle");

                self.next_reading = Instant::now() + self.delta_time;

                None
            }
            State::AnalyzingData => {
                println!("Analyzing data");
                // so we have a bunch of data. Now lets simulate and generate similar data
                self.state = State::Done;
                None
            }
            State::Done => {
                println!("Auto-tuner complete");
                self.parameters
            }
        }
    }

    fn capture(&mut self) {}
}
