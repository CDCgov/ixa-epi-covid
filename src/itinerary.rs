use ixa::{HashMap, csv, prelude::*};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{parameters::{ContextParametersExt, Params}, population_loader::PersonId};

use ixa::profiling::open_span;

define_entity!(Itinerary);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PersonItinerary(PersonId);
define_property!(PersonItinerary);
