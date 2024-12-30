pub type Bar = f32;
pub type Temperature = f32;
pub type Watts = f32;
pub type Grams = f32;
pub type Degrees = f32;
pub type MPa = f32;

fn from_bar_to_mpa(bar: Bar) -> MPa {
    bar / 10.0
}

fn from_mpa_to_bar(mpa: MPa) -> Bar {
    mpa * 10.0
}
