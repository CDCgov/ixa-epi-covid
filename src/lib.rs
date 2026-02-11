pub mod model;
pub mod parameters;

pub use model::initialize_model;
pub use parameters::ParametersContextExt;
pub use parameters::Params;

mod pyepicovid;
pub use pyepicovid::epimodel::EpiModel;
