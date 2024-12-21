use std::time::Duration;

#[derive(Debug, Copy, Clone)]
pub struct DataPoint {
    delta_t: Duration,
    power: f32,
    probe_temperature: f32,
}

pub struct ObservedData {
    data: Vec<DataPoint>,
}

impl ObservedData {
    pub fn new(samples: Option<Vec<(f32, f32, f32)>>) -> Self {
        let data = samples.unwrap_or_else(|| crate::models::samples::OBSERVED_DATA.to_vec());
        data.as_slice().into()
    }

    pub fn apply_noise(&mut self) {
        use rand::prelude::*;
        let distribution = rand_distr::Normal::new(0.0, 1.0).unwrap();
        self.data.iter_mut().for_each(|point| {
            let noise: f32 = distribution.sample(&mut thread_rng());
            point.probe_temperature += noise;
        });
    }

    pub fn apply_smoothing(&mut self, window_size: usize) {
        let mut smoothed_data = Vec::new();

        for i in 0..self.data.len() {
            let start = if i < window_size { 0 } else { i - window_size };
            let end = if i + window_size >= self.data.len() {
                self.data.len()
            } else {
                i + window_size
            };
            let window = &self.data[start..end];
            let average_temperature = window
                .iter()
                .map(|point| point.probe_temperature)
                .sum::<f32>()
                / window.len() as f32;

            smoothed_data.push(DataPoint {
                delta_t: self.data[i].delta_t,
                power: self.data[i].power,
                probe_temperature: average_temperature,
            });
        }

        self.data = smoothed_data;
    }

    pub fn get_control_vector(&self) -> Vec<(f32, f32)> {
        self.data
            .iter()
            .map(|point| (point.delta_t.as_secs_f32(), point.power))
            .collect()
    }

    pub fn get_measurements(&self) -> Vec<f32> {
        self.data
            .iter()
            .map(|point| point.probe_temperature)
            .collect()
    }

    fn get_longest_powered_slice(&self) -> &[DataPoint] {
        let mut max_start = 0;
        let mut max_len = 0;
        let mut current_start = 0;
        let mut current_len = 0;

        for (i, point) in self.data.iter().enumerate() {
            if point.power > 0.0 {
                if current_len == 0 {
                    current_start = i;
                }
                current_len += 1;

                if current_len > max_len {
                    max_len = current_len;
                    max_start = current_start;
                }
            } else {
                current_len = 0;
            }
        }

        &self.data[max_start..max_start + max_len]
    }

    fn get_longest_negative_slice(&self) -> &[DataPoint] {
        let mut max_start = 0;
        let mut max_len = 0;
        let mut current_start = 0;
        let mut current_len = 0;

        self.data.windows(2).enumerate().for_each(|(i, window)| {
            let (prev, next) = (window[0], window[1]);
            if prev.probe_temperature > next.probe_temperature
                && prev.power == 0.0
                && next.power == 0.0
            {
                if current_len == 0 {
                    current_start = i;
                    current_len = 1;
                }
                current_len += 1;

                if current_len > max_len {
                    max_len = current_len;
                    max_start = current_start;
                }
            } else {
                current_len = 0;
            }
        });

        &self.data[max_start..max_start + max_len]
    }

    pub fn estimate_ambient_transfer_coefficient(
        &self,
        ambient_temperature: f32,
        thermal_mass: f32,
    ) -> Option<f32> {
        let slice = self.get_longest_negative_slice();
        println!("Slice length: {}", slice.len());
        // need a run of at least a minute
        if slice.len() < 60 {
            return None;
        }

        let mut ks = Vec::new();

        for i in 0..slice.len() - 1 {
            let delta_temp = slice[i + 1].probe_temperature - slice[i].probe_temperature; // degrees
            let temperature_difference = slice[i].probe_temperature - ambient_temperature; // degrees
            let delta_time = slice[i + 1].delta_t.as_secs_f32(); //seconds
            let delta_temp_dt = delta_temp / delta_time; // degrees per second

            if temperature_difference > 0.0 {
                // J/K/s^-1 -> W/K
                ks.push(-thermal_mass * delta_temp_dt / temperature_difference);
            }
        }

        if ks.len() == 0 {
            return None;
        }
        Some(ks.iter().sum::<f32>() / ks.len() as f32)
    }
}

impl From<&[(f32, f32, f32)]> for ObservedData {
    fn from(data: &[(f32, f32, f32)]) -> Self {
        let mut last_instant: f32 = data[0].0;

        let data = data
            .into_iter()
            .map(|(t, power, probe_temperature)| {
                //introduce 0-4 degrees of noise
                let point = DataPoint {
                    delta_t: Duration::from_secs_f32(t - last_instant),
                    power: *power,
                    probe_temperature: *probe_temperature,
                };
                last_instant = *t;
                point
            })
            .collect();

        Self { data }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_ambient_transfer_coefficient() {
        let mut observed_data = ObservedData::new(None);
        observed_data.apply_noise();
        observed_data.apply_smoothing(3);

        let ambient_transfer_coefficient =
            observed_data.estimate_ambient_transfer_coefficient(40.0, 1300.0);

        println!(
            "Ambient transfer coefficient: {:?}",
            ambient_transfer_coefficient
        );
        assert!(ambient_transfer_coefficient.is_some());
        assert!((ambient_transfer_coefficient.unwrap() - 0.8).abs() < 0.1);
    }

    #[test]
    fn test_get_longest_negative_slice() {
        let data = vec![
            (0.0, 0.0, 100.0),
            (1.0, 0.0, 95.0),
            (2.0, 0.0, 90.0),
            (3.0, 0.0, 85.0),
            (4.0, 0.0, 80.0),
            (5.0, 0.0, 85.0),
            (6.0, 0.0, 80.0),
            (7.0, 0.0, 75.0),
            (8.0, 0.0, 70.0),
        ];
        let observed_data: ObservedData = ObservedData {
            data: data
                .into_iter()
                .map(|(t, p, tp)| DataPoint {
                    delta_t: Duration::from_secs_f32(t),
                    power: p,
                    probe_temperature: tp,
                })
                .collect::<Vec<DataPoint>>(),
        };

        let longest_negative_slice = observed_data.get_longest_negative_slice();
        assert_eq!(longest_negative_slice.len(), 5);
        assert_eq!(longest_negative_slice[0].probe_temperature, 100.0);
        assert_eq!(longest_negative_slice[4].probe_temperature, 80.0);

        let data = vec![
            (0.0, 0.0, 80.0),
            (1.0, 0.0, 100.0),
            (2.0, 0.0, 95.0),
            (3.0, 0.0, 90.0),
            (4.0, 0.0, 85.0),
            (5.0, 0.0, 80.0),
            (6.0, 0.0, 85.0),
            (7.0, 0.0, 80.0),
            (8.0, 0.0, 75.0),
            (9.0, 0.0, 70.0),
        ];
        let observed_data: ObservedData = ObservedData {
            data: data
                .into_iter()
                .map(|(t, p, tp)| DataPoint {
                    delta_t: Duration::from_secs_f32(t),
                    power: p,
                    probe_temperature: tp,
                })
                .collect::<Vec<DataPoint>>(),
        };

        let longest_negative_slice = observed_data.get_longest_negative_slice();
        assert_eq!(longest_negative_slice.len(), 5);
        assert_eq!(longest_negative_slice[0].probe_temperature, 100.0);
        assert_eq!(longest_negative_slice[4].probe_temperature, 80.0);
    }
}
