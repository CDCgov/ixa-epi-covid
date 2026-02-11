use crate::model;
use crate::parameters::{ParametersContextExt, Params};
use ixa::{Context, ContextGlobalPropertiesExt, ContextReportExt};
use pyo3::prelude::*;
use std::path::PathBuf;

#[pyclass]
pub struct EpiModel {
    config_file: PathBuf,
    force_overwrite: bool,
}

#[pymethods]
impl EpiModel {
    #[new]
    fn new(config_file: PathBuf, force_overwrite: bool) -> Self {
        EpiModel {
            config_file,
            force_overwrite,
        }
    }

    fn run(&self) -> PyResult<()> {
        {
            // scope for Context so files are closed before reading them
            let context = &mut Context::new();

            context
                .load_global_properties(&self.config_file)
                .expect("Failed to load global properties");
            let report_config = context.report_options();
            report_config.overwrite(self.force_overwrite);

            let &Params { seed, max_time, .. } = context.get_params();
            model::initialize_model(context, seed, max_time).expect("Model initialization failed");
            context.execute();
        }
        Ok(())
    }
}

#[pymodule]
pub mod epimodel {
    #[pymodule_export]
    use super::EpiModel;
}
