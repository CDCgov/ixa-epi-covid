use ixa::IxaError;
use thiserror::Error;

use crate::pop_reader::errors::PopulationReaderError;

#[derive(Error, Debug)]
pub enum ModelError {
    #[error("Model Error: {0}")]
    ModelError(String),

    #[error(transparent)]
    IxaError(#[from] IxaError),

    #[error(transparent)]
    StringError(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    ParseIntError(#[from] std::num::ParseIntError),

    #[error(transparent)]
    IxaCsvError(#[from] ixa::csv::Error),

    #[error(transparent)]
    PopulationReaderError(#[from] PopulationReaderError),
}
