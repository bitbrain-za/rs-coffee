// [ ] Go through all the "expects" and change them to put the system into an error/panic state
// [ ] Remove this later, just silence warnings while we're doing large scale writing
#![allow(dead_code)]
mod app_state;
mod board;
mod config;
mod gpio;
mod indicator;
mod kv_store;
mod models;
mod sensors;
mod state_machines;
use anyhow::Result;
use app_state::System;
use board::{Action, F32Read, Reading};
use state_machines::operational_fsm::OperationalState;
use state_machines::system_fsm::{SystemState, Transition as SystemTransition};
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Starting up");

    match run_boiler_optimisation() {
        Err(e) => {
            println!("{e}");
            std::process::exit(1);
        }
        Ok(results) => {
            let actual: [f32; 3] = [1255.8, 0.8, 0.0125];
            let error = results
                .iter()
                .zip(actual.iter())
                .map(|(a, b)| (a - b).abs() / b * 100.0)
                .collect::<Vec<f32>>();

            println!("Actual values: {:?}", actual);
            println!("Predicted values: {:?}", results);
            println!("Error Percent: {:?}", error);
        }
    }

    log::info!("Setup complete, starting main loop");
    let system = System::new();

    {
        let board = system.board.lock().unwrap();
        *board.outputs.boiler_duty_cycle.lock().unwrap() = 0.5;
        *board.outputs.pump_duty_cycle.lock().unwrap() = 0.2;
        *board.outputs.solenoid.lock().unwrap() =
            gpio::relay::State::on(Some(Duration::from_secs(5)));
    }
    let system = System::new();

    system
        .system_state
        .lock()
        .unwrap()
        .transition(SystemTransition::Idle)
        .expect("Invalid transition :(");
    loop {
        thread::sleep(Duration::from_millis(1000));
    }

    let mut loop_interval = Duration::from_millis(1000);
    loop {
        let system_state = system.system_state.lock().unwrap().clone();
        let operational_state = system.operational_state.lock().unwrap().clone();

        match (system_state, operational_state) {
            (SystemState::Healthy, operational_state) => {
                let boiler_temperature = system.read_f32(F32Read::BoilerTemperature);
                let pump_pressure = system.read_f32(F32Read::PumpPressure);
                println!("Boiler temperature: {}", boiler_temperature);
                println!("Pump pressure: {}", pump_pressure);
                println!("Weight: {}", system.read_f32(F32Read::ScaleWeight));

                match operational_state {
                    OperationalState::Idle => {
                        let _ = system.execute_board_action(Action::SetIndicator(
                            indicator::ring::State::Idle,
                        ));
                    }
                    OperationalState::Brewing => {
                        let indicator = indicator::ring::State::Temperature {
                            min: 25.0,
                            max: 100.0,
                            level: boiler_temperature,
                        };
                        let _ = system.execute_board_action(Action::SetIndicator(indicator));
                    }
                    OperationalState::Steaming => {
                        let indicator = indicator::ring::State::Temperature {
                            min: 25.0,
                            max: 140.0,
                            level: boiler_temperature,
                        };
                        let _ = system.execute_board_action(Action::SetIndicator(indicator));
                    }
                    OperationalState::AutoTuning(_, _) => {
                        log::info!("Autotuning in progress");
                        log::info!("{}", operational_state);

                        /* We have to run the device for a bit and collect it's dynamics
                           once that's expired, we do a curve fit to get the parameters
                           for the boiler model.

                           Step 1. Run for an hour collecting data
                           Step 2. Fit curve
                           Step 3. Update parameters
                        */
                        loop_interval = Duration::from_secs(1);

                        let indicator =
                            if let Some(percentage) = operational_state.percentage_complete() {
                                log::info!("Autotuning is {:.2}% complete", percentage);
                                indicator::ring::State::Guage {
                                    min: 0.0,
                                    max: 100.0,
                                    level: percentage,
                                }
                            } else {
                                log::error!("Couldn't get percentage completed");
                                indicator::ring::State::Temperature {
                                    min: 25.0,
                                    max: 150.0,
                                    level: boiler_temperature,
                                }
                            };
                        let _ = system.execute_board_action(Action::SetIndicator(indicator));
                    }
                    _ => {}
                }
            }
            (SystemState::Error(message), _) => {
                log::error!("System is in an error state: {}", message);
            }
            (SystemState::Panic(message), _) => {
                log::error!("System is in a panic state: {}", message);
            }

            (_, _) => {
                log::error!("unhandled state")
            }
        }

        if let Reading::AllButtonsState(Some(presses)) =
            system.do_board_read(Reading::AllButtonsState(None))
        {
            if !presses.is_empty() {
                for button in presses {
                    println!("Button pressed: {}", button);

                    if button == board::ButtonEnum::Brew {
                        let _ = system
                            .execute_board_action(Action::OpenValve(Some(Duration::from_secs(5))));
                    }
                    if button == board::ButtonEnum::HotWater {
                        system.error("Dummy error".to_string());
                    }
                    if button == board::ButtonEnum::Steam {
                        system.panic("Dummy panic".to_string());
                    }
                }
            }
        }
        thread::sleep(loop_interval);
    }
}

use argmin::{
    core::{CostFunction, Error, Executor},
    solver::neldermead::NelderMead,
};
use models::data_manipulation::ObservedData;
use ndarray::{array, Array1};

const AMBIENT_TEMPERATURE: f64 = 40.0;

fn boiler_dynamic(
    thermal_mass: f64,
    ambient_transfer_coefficient: f64,
    probe_transfer_coefficient: f64,
    boiler_temperature: f32,
    probe_temperature: f32,
    power: f32,
    dt: std::time::Duration,
) -> (f32, f32) {
    // Boiler temperature change without flow heat loss
    let boiler_temperature = boiler_temperature as f64;
    let probe_temperature = probe_temperature as f64;
    let power = power as f64;
    let dt = dt.as_secs_f64();
    let d_temp_d_time_boiler = (power
        - (ambient_transfer_coefficient * (boiler_temperature - AMBIENT_TEMPERATURE)))
        / thermal_mass;
    let delta_boiler = d_temp_d_time_boiler * dt;

    // Probe temperature change (dependent on boiler temperature)
    let d_temp_d_time_probe = probe_transfer_coefficient * (boiler_temperature - probe_temperature);
    let delta_probe = d_temp_d_time_probe * dt;

    (
        (boiler_temperature + delta_boiler) as f32,
        (probe_temperature + delta_probe) as f32,
    )
}

fn generate_predictions(
    thermal_mass: f64,
    ambient_transfer_coefficient: f64,
    probe_transfer_coefficient: f64,
    boiler_temperature: f32,
    probe_temperature: f32,
    power: &[f32],
    dt: &[std::time::Duration],
) -> Vec<(f32, f32)> {
    assert_eq!(power.len(), dt.len());

    let mut last_boiler_temp = boiler_temperature;
    let mut last_probe_temp = probe_temperature;

    power
        .iter()
        .zip(dt.iter())
        .map(|(p, t)| {
            let (boiler_temp, probe_temp) = boiler_dynamic(
                thermal_mass,
                ambient_transfer_coefficient,
                probe_transfer_coefficient,
                last_boiler_temp,
                last_probe_temp,
                *p,
                *t,
            );

            last_boiler_temp = boiler_temp;
            last_probe_temp = probe_temp;

            (boiler_temp, probe_temp)
        })
        .collect()
}

fn predict(input: &[f32], parameters: &[f64], independant_variables: &[(f32, f32)]) -> Vec<f32> {
    assert_eq!(input.len(), 2);
    assert_eq!(parameters.len(), 3);
    let thermal_mass = parameters[0];
    let ambient_transfer_coefficient = parameters[1];
    let probe_transfer_coefficient = parameters[2];
    let boiler_temperature = input[0];
    let probe_temperature = input[1];
    let power: Vec<f32> = independant_variables.iter().map(|(_, p)| *p).collect();
    let dt: Vec<Duration> = independant_variables
        .iter()
        .map(|(t, _)| Duration::from_secs_f32(*t))
        .collect();

    generate_predictions(
        thermal_mass,
        ambient_transfer_coefficient,
        probe_transfer_coefficient,
        boiler_temperature,
        probe_temperature,
        &power,
        &dt,
    )
    .iter()
    .map(|(_, probe)| *probe)
    .collect()
}

pub struct Params {}

struct BoilerProblem {
    input: Vec<f32>,
    observed_data: ObservedData,
    independant_variables: Vec<(f32, f32)>,
    initial_guess: [f32; 3],
    confidence: [f32; 3],
}

type SimplexArray = ndarray::ArrayBase<ndarray::OwnedRepr<f64>, ndarray::Dim<[usize; 1]>>;
impl BoilerProblem {
    const DEFAULT_GUESS: [f32; 3] = [4186.0, 1.8, 0.25];
    fn new(guess: Option<[f32; 3]>, confidence: Option<[f32; 3]>) -> Self {
        let guess = guess.unwrap_or(Self::DEFAULT_GUESS);
        let confidence: [f32; 3] = confidence.unwrap_or([0.5, 0.1, 0.1]);
        let observed_data = ObservedData::new(None);
        let initial_temperature = observed_data.get_measurements()[0];
        let input = vec![initial_temperature, initial_temperature];
        let independant_variables = observed_data.get_control_vector();
        Self {
            input,
            observed_data,
            independant_variables,
            initial_guess: guess,
            confidence,
        }
    }

    pub fn initial_guess(&self) -> SimplexArray {
        array![
            self.initial_guess[0] as f64,
            self.initial_guess[1] as f64,
            self.initial_guess[2] as f64,
        ]
    }
    pub fn perturb_thermal_mass(&self) -> SimplexArray {
        array![
            (self.initial_guess[0] * self.confidence[0]) as f64,
            self.initial_guess[1] as f64,
            self.initial_guess[2] as f64,
        ]
    }
    pub fn perturb_ambient_transfer_coefficient(&self) -> SimplexArray {
        array![
            self.initial_guess[0] as f64,
            (self.initial_guess[1] * self.confidence[1]) as f64,
            self.initial_guess[2] as f64,
        ]
    }
    pub fn perturb_probe_transfer_coefficient(&self) -> SimplexArray {
        array![
            self.initial_guess[0] as f64,
            self.initial_guess[1] as f64,
            (self.initial_guess[2] * self.confidence[2]) as f64,
        ]
    }

    pub fn get_simplex(&self) -> Vec<SimplexArray> {
        vec![
            self.initial_guess(),
            self.perturb_thermal_mass(),
            self.perturb_ambient_transfer_coefficient(),
            self.perturb_probe_transfer_coefficient(),
        ]
    }
}

impl CostFunction for BoilerProblem {
    type Param = Array1<f64>;
    type Output = f64;

    fn cost(&self, p: &Self::Param) -> Result<Self::Output, Error> {
        let params = p.to_vec();
        let predicted_data = predict(&self.input, &params, &self.independant_variables);

        let cost = self
            .observed_data
            .get_measurements()
            .iter()
            .zip(predicted_data.iter())
            .map(|(observed, predicted)| {
                let dif = (*observed - *predicted) as f64;
                dif * dif
            })
            .sum::<f64>();

        let cost = cost / self.observed_data.get_measurements().len() as f64;

        Ok(cost)
    }
}

fn run_boiler_optimisation() -> Result<Vec<f32>, Error> {
    let mut problem = BoilerProblem::new(None, None);

    log::info!("adding noise");
    problem.observed_data.apply_noise();

    log::info!("creating solver");
    let solver = NelderMead::new(problem.get_simplex())
        .with_alpha(1.0)?
        .with_gamma(8.0)?
        .with_rho(0.5)?
        .with_sigma(0.99)?;

    log::info!("solving...");
    let res = Executor::new(problem, solver)
        .configure(|state| state.max_iters(1000))
        .run()?;

    println!("{}", res);
    let best_params = res.state.best_param.unwrap();
    let best_params = best_params
        .to_vec()
        .iter()
        .map(|x| *x as f32)
        .collect::<Vec<f32>>();
    println!("Best parameters: {:?}", best_params);
    Ok(best_params)
}
