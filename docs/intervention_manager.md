# Intervention Module

This module manages geographically targeted interventions that affect peoples itineraries. Supported interventions are:

- `SchoolClosure`
- `WorkplaceMobilityReduction`
- `CommunityMobilityReduction`

## Defining an Intervention from Input
The interventions field in the `input.json` is an array and users can define multiple interventions within it. Each intervention is specified by a `Intervention` struct with the following fields.

| Field | Description |
|---|---|
| `modifier` | The intervention type. |
| `geography` | The state or county where the intervention applies. This is specified by a state or county FIPS code|
| `acceptance_probability` | Probability that an eligible person accepts the intervention |
| `activation_time` | Simulation time when the intervention starts |
| `duration` | Optional duration, expressed in the same units as simulation time |
| `override_modifiers` | Optional changes to the default itinerary modifier specification |


## Person Properties
We create a fixed length array person property, `AcceptsIntervention`. It is length of the `Modifier` enum by the length of the `geography` enum. Each entry in this array is an optional geography enum value. Note the geography enum encodes the FIPS code associated with the geography type. Examples are `State(18)` and `County(18067)`. If an entry is `Some()` in this array then the person accepts that modifier for that geography level. Derived person properties are created for each combination of modifier, geography, and acceptance of the intervention by looking at each element of `AcceptsIntervention`. These person properties are what are registered with the itinerary manager.


## Triggering Interventions
Given `activates_at` and `duration`, ixa `Triggers` are established in the `setup_intervention_triggers(Intervention)` method. These triggers emit an `InterventionEvent` which contains an `active: bool` value and a copy of the intervention struct. When the `active` is `True` the intervention has just begun and when false the intervention ends.

In the method `setup_intervention_trigger_event_subscription` event subscriptions are established to listen for `InterventionEvent`s. When an active intervention event is received several steps occur that use the information in the intervention struct.
1. The individual level acceptance of the intervention is determined via the `set_accepts_intervention(Intervention)` method. This matches on the `(intervention.modifier, intervention.geography)` and queries the people that could be affected by the intervention. These people are stochastically determined to follow the intervention with probability `intervention.acceptance_probability`. The `AcceptsIntervention` person property is modified if the person accepts the intervention
2. The itinerary transmission matrix is created for the corresponding `intervention.modifier`. The itinerary transition matrix is stored in the itinerary manager with a person property that is determined by matching on `(intervention.modifier, intervention.geography)`. These person properties are the derived person properties created from the `AcceptsIntervention` person property
3. The `InterventionData` data plugin adds the intervention information and to the data structure `HashMap<(intervention.modifier, intervention.geography), bool>`.

If an inactive intervention is received
1. The itinerary transmission matrix is removed from the itinerary manager. The removal by person property is determined by matching on `(intervention.modifier, intervention.geography)`.
2. The `InterventionData` data plugin removes the intervention information and to the data structure `HashMap<(intervention.modifier, intervention.geography), bool>`.


### Creating itinerary transition matrices
Depending on the type of intervention and any override information, we create an itinerary transition matrix for the intervention. We assume that school closures and workplace mobility reductions impact peoples locations, and community mobility reductions impact peoples activities. We encode all geographic heirarchy into the acceptance function of the itinerary transition matrix.
