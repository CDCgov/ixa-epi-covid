# Simulation Initialization

## Seeding Initial Conditions
When the simulation is instantiated, all individuals are created in the susceptible compartment. In order to seed infections in the model outside of transmission, the user can supply an initial prevalence of infection. The infectious seeding process samples from the susceptible population using a binomial distribution with success probability of `initial_prevalence`, such that the actual initial prevalence of infectious individuals at time 0.0 is sampled with some noise around the provided parameter. The realized initial prevalence is evaluated with respect to exactly time 0.0, when transmission is activated in the model. Seeded infections therefore do not produce novel infection attempts until time 0.0.

Seeded infections are allowed to vary in their remaining duration of infection at time 0.0. This is implemented using negative simulation time feature of `ixa`, where individuals to be seeded as infectious sample their infection duration elapsed at time 0.0 to be uniformly distributed from 0 to 100% of their assigned infection duration. Note that the process of seeding infections is distinct from the importation of novel infections from outside the community, discussed in more detail in the [importation module](importation.md). Even though both approaches allow for infectious individuals who arise external to the transmission model, imported cases begin their infection upon importation and still have their whole duration of infection ahead of them.

## Synthetic populations
A synthetic population is a structured `.csv` file that defines the population
to be simulated. Each row corresponds to an individual with four columns:
`age`, `homeId`, `schoolId`, `workplaceId`. The input format is unchanged.

- `age`: age of the individual.
- `homeId`, `schoolId`, `workplaceId`: setting codes for the individual's home,
  school, and work. Home is required; school/work may be empty.

The model now stores all settings in a single `Setting` entity with categories
`Home`, `School`, `Work`, and `Community`. Memberships are recorded via
`PersonSetting` edges. `population_loader.rs` still reads the same columns and
creates one setting per category/code pair, adding each person to the
appropriate settings.

Community membership is derived from `homeId`. The first 11 characters of
`homeId` define the tract (FIPS state/county/tract); the remaining 6 characters
define the group within that tract. The loader creates a `Community` setting
using the 11-character prefix and joins the person to it.

All setting codes in the CSV should be 17-character numeric strings. These are
parsed into integers for storage and querying; keep the structured format so
both tract and group remain accessible.
