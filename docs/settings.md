# Settings
The settings module defines the contact structure of the model. It stores which
settings exist, which people belong to them, and how those memberships affect
transmission.

## Setting Representation
Settings are represented explicitly as entities. Each `Setting` has:

- A `SettingCategory`: `Home`, `School`, `Work`, or `Community`
- A `SettingCode`: the identifier parsed from the synthetic population input
- An `Alpha`: the density-dependence parameter for that setting

Membership is represented separately through `PersonSetting` entities. Each
`PersonSetting` edge links one person to one setting. This replaces the older
approach that stored itineraries and setting membership in custom maps.

The model assumes that `alpha` is uniform within a category. In practice,
`population_loader` reads category-level `SettingProperties` from model input
and uses them when creating settings from the synthetic population.

## Active Settings For A Person
Each person is active in the settings for which they have `PersonSetting`
memberships. There is no longer a stored per-person itinerary object. Instead,
the current active settings are recovered by querying the `PersonSetting`
membership table.

The synthetic population loader creates memberships as follows:

- Every person is added to one `Home` setting
- Every person is added to one `Community` setting derived from the first 11
  digits of `homeId`
- A person may also be added to one `School` setting
- A person may also be added to one `Work` setting

The weights associated with setting categories are still configured globally in
`itinerary_ratios`. These values now mean "relative contact weight by category"
rather than entries in a separately stored itinerary vector.

## Relationship To Transmission
Each setting contributes a multiplier based on its size and category-specific
alpha:

`(N - 1)^alpha`

where `N` is the number of people in the setting.

Two related quantities are derived from a person's active settings:

- Current infectiousness multiplier:
  a weighted average of setting multipliers using `itinerary_ratios`,
  normalized over the person's active settings
- Maximum infectiousness multiplier:
  the maximum setting-specific multiplier across that person's active settings

The current multiplier is used to scale realized infectiousness. The maximum
multiplier is used when forecasting the next possible transmission event so the
rejection-sampling step remains valid.

When the model samples which setting an infectious person attempts transmission
through, it weights each active setting by:

`itinerary_ratio * (N - 1)^alpha`

If all such weights are zero, the model falls back to sampling uniformly across
the person's active settings.

## Comparison To Older Design
The older implementation used generic setting IDs, trait objects, and an
explicit itinerary container. The current design removes that abstraction layer
in favor of:

- One `Setting` table for all setting categories
- One `PersonSetting` membership table
- Category-level configuration through `SettingCategory`
- Query-based recovery of active memberships

This makes the settings model closer to the data it represents and easier to
benchmark against older implementations.
