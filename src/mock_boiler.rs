pub struct BiolerProperties {
    pub thermal_mass: f32,
    pub heat_loss_coefficient: f32,
    pub wattage: f32,
    pub probe_time_constant: f32,
}

impl BiolerProperties {
    const HEAT_CAPACITY_WATER: f32 = 4186.0; // J/gC
}

impl Default for BiolerProperties {
    fn default() -> Self {
        let mass = 0.5;
        Self {
            thermal_mass: Self::HEAT_CAPACITY_WATER * mass,
            heat_loss_coefficient: 0.8,
            wattage: 1150.0,
            probe_time_constant: 20.0,
        }
    }
}

pub struct MockBoiler {
    temperature: f32,
    probe_temperature: f32,
    ambient_temp: f32,
    control: f32,
    last_update: std::time::Instant,
    update_window: std::time::Duration,
    properties: BiolerProperties,
}

impl MockBoiler {
    pub fn new(ambient_temp: f32) -> Self {
        Self {
            temperature: ambient_temp,
            probe_temperature: ambient_temp,
            ambient_temp,
            control: 0.0,
            last_update: std::time::Instant::now(),
            update_window: std::time::Duration::from_secs(1),
            properties: BiolerProperties::default(),
        }
    }

    pub fn tick(&mut self) {
        if std::time::Instant::now() - self.last_update < self.update_window {
            return;
        }
        self.update();
    }

    pub fn ambient_loss_dt(&self) -> f32 {
        (self.temperature - self.ambient_temp) * self.properties.heat_loss_coefficient
    }

    pub fn update(&mut self) {
        let dt = self.last_update.elapsed().as_secs_f32();
        self.last_update = std::time::Instant::now();

        // First order lag response for probe
        self.probe_temperature += ((self.temperature - self.probe_temperature)
            / self.properties.probe_time_constant)
            * dt;

        let q = self.control * self.properties.wattage;
        let d_temperature_dt = (q - self.ambient_loss_dt()) / self.properties.thermal_mass;
        let delta_temp = (d_temperature_dt) * dt;
        self.temperature += delta_temp;
    }

    pub fn set_control(&mut self, control: f32) {
        if self.control != control {
            self.update();
            self.control = control;
        }
    }

    pub fn get_temperature(&self) -> f32 {
        log::info!(
            "Probe: {}C, Boiler: {}C",
            self.probe_temperature,
            self.temperature
        );
        self.probe_temperature
    }
}
