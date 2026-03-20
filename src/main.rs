//use epimodel::{ContextParametersExt, Params, initialize_model};

use epimodel::model::initialize_model;
use ixa::{prelude::*};

fn main() {
    let mut context = Context::new();
    initialize_model(&mut context, 123, 100.0).expect("Model initialization failed");
}
