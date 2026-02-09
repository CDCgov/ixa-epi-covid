# Model Input
The model's behavior is defined by several input parameters which are listed below. Parameter constraints are not listed here but can be identified in `parameters.rs`.

#### `seed`
The seed of the model's random number generator

#### `max_time`
The time the simulation terminates. Any plans scheduled later than `max_time` will not occur. If all plans are completed before `max_time` occurs, the simulation will terminate.

#### `synth_population_file`
Path to the synthetic population file. This file informs the underlying population characteristics and contact structure. See [simulation initialization documentation](initialization.md) for more detail.

#### `initial_incidence`
The proportion of people that begin the simulation in the infectious state. See [simulation initialization documentation](initialization.md) for more detail.

#### `infectiousness_rate_fn`
A library of infection rates assigned to individual when they become infectious. Possible values are `EmpiricalFromFile`, which requires a file of rates and a numeric scale value, and `Constant`, which requires a rate and duration See [transmission documentation](transmission.md) for more detail. Example data can be found in `input/library_empirical_rate_fns.csv`.

#### `setting_properties`

This parameter struct defines a map of `CoreSettingsTypes` and `SettingProperties`. There must be alignment between the settings enumerated in this struct and the settings that are declared in the model instantiation. With each setting type, the following attributes must be defined in the `SettingProperties`:
- `alpha` parameter informing density-dependent transmission in the setting. Density-dependent transmission is a multiplier on an individual's infectiousness
- `itinerary_specification` parameter used to define the proportion of time an individual spends in the setting

See the [settings documentation](settings.md) for more details.

### `prevalence_report`
This is defined by a `ReportParams` struct and creates the report indicating the number of individuals in infectious, symptomatic, and hospitalized compartments each day of the simulation.

### `incidence_report`
This is defined by a `ReportParams` struct and creates the report indicating the number of incident transitions of the infectious, symptomatic, and hospitalized progressions each day of the simulation.

### `transmission_report`
This is defined by a `ReportParams` struct and creates the report tracking the individuals and location of each accepted infection attempt.

See the [reports documentation](reports.md) for more details on all report types.
