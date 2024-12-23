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

pub struct AutoTuner {
    state: State,
    setpoint: f32,
    boiler_model: BoilerModel,
    parameters: Option<BoilerModelParameters>,
    start_time: Instant,
    duration: Duration,
    delta_time: Duration,
    data_points: Vec<DataPoint>,
    next_reading: Instant,
    system: System,
}

impl AutoTuner {
    pub fn new(setpoint: f32, system: System) -> Self {
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
                log::info!("Setting up auto-tuner");
                self.state = State::GatheringData;
                None
            }
            State::GatheringData => {
                if self.next_reading > Instant::now() {
                    return None;
                }

                None
            }
            State::AnalyzingData => {
                log::info!("Analyzing data");
                // so we have a bunch of data. Now lets simulate and generate similar data
                self.state = State::Done;
                None
            }
            State::Done => {
                log::info!("Auto-tuner complete");
                self.parameters
            }
        }
    }

    fn capture(&mut self) {}
}
