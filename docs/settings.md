# Settings
The setting module provides the framework governing the contact structure of individuals in the model where transmission can occur.

## Setting Definition
A setting is defined by `SettingId` and a set of `SettingProperties`. A `SettingId` contains the setting category (e.g., home, school, work, etc.) and a unique identifier within the given category. Each setting category is associated with `SettingProperties` which contain a parameter for density dependent transmission `alpha`, and `itinerary_specification` which defines the proportion of time an individual interacts in the setting category. This value is also referred to as a ratio. Setting properties are assigned for each setting category in [model input](model-input.md). It is assumed that setting properties are uniform across all settings of a certain type. Settings are implemented with the `AnySettingId` trait, which is referenced throughout the implementation when working with generic setting objects.

## Itineraries and Itinerary Modifiers
Itineraries are a vector of `ItineraryEntry` which store a setting an
individual is a member of and a ratio of time spent in the setting. By default,
the ratio values for itinerary values are those given in `SettingProperties`
input for the corresponding setting category. Itineraries are stored in the
`SettingsDataContainer` as map between the `PersonId` and itinerary. Upon model
initialization, an individual's default itinerary is generated from the
synthetic population loader module, where rows of the synthetic population
correspond to the setting IDs for a specific person (see [initialization
documentation](initialization.md) for more details). The codebase is designed
with a specific set of settings in mind. Four `CoreSettingTypes` are
implemented: Home, School, Work, and Community. There is a required
correspondence between the setting categories listed in `SettingProperties`
input and the structure of the synthetic population file. An example of an
individual's itinerary is {Home – ID: 1, ratio: 0.33; School – ID: 1, ratio:
0.33; Community – ID: 1, ratio: 0.33}

### Relationship to transmission
Setting properties also impact underlying infection attempt process. Each setting category has a density dependent transmission parameter $\alpha$. These $\alpha$ values are parameters used in the individual level infectiousness multipliers that take the form $(N-1)^\alpha$ where $N$ is the number of people in the setting and $\alpha \in [0,1]$. How these multipliers are used to implement rejection sampling is discussed further in the [transmission module documentation](transmission.md).
