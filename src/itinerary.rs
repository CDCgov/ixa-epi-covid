use ixa::{HashMap, csv, prelude::*};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{parameters::{ContextParametersExt, Params}, population_loader::{PersonId}};

use ixa::profiling::open_span;

define_entity!(Itinerary);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
struct PersonItinerary(PersonId);
impl_property!(PersonItinerary, Itinerary);
