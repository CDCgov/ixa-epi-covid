# Simulation Initialization

## Seeding Initial Conditions
When the simulation is instantiated, all individuals are created in the susceptible compartment. In order to seed infections in the model outside of transmission, the user can supply an initial prevalence of infection. The infectious seeding process samples from the susceptible population using a binomial distribution with success probability of `initial_prevalence`, such that the actual initial prevalence of infectious individuals at time 0.0 is sampled with some noise around the provided parameter. The realized initial prevalence is evaluated with respect to exactly time 0.0, when transmission is activated in the model. Seeded infections therefore do not produce novel infection attempts until time 0.0.

Seeded infections are allowed to vary in their remaining duration of infection at time 0.0. This is implemented using negative simulation time feature of `ixa`, where individuals to be seeded as infectious sample their infection duration elapsed at time 0.0 to be uniformly distributed from 0 to 100% of their assigned infection duration. Note that the process of seeding infections is distinct from the importation of novel infections from outside the community, discussed in more detail in the [importation module](importation.md). Even though both approaches allow for infectious individuals who arise external to the transmission model, imported cases begin their infection upon importation and still have their whole duration of infection ahead of them.

## Synthetic populations
A synthetic population is a structured `.csv` file which defines the population that will be simulated. Each row corresponds to an individual with the properties defined by the columns of the file: `age`, `homeId`, `schoolId`, `workplaceId`. `age` corresponds to the age of the individual. `homeId`, `schoolId`, and `workplaceId` correspond to the home, school, and workplace settings an individual belongs to. An individual must belong to a home setting, but does not need to belong to a school or workplace (this is indicated by an empty entry). An individual's community or census tract group is derived from the individual's `homeId`. The implementation in `population_loader.rs` adds all people to the model, assigns the age person property and setting itinerary to each individual.

The model now expects ASPR-compatible setting identifiers. The geographic prefix remains fixed-width, but the observed ASPR data sometimes uses one extra decimal digit in the sequential suffix when the sequence value exceeds the originally documented width:

- `homeId`: 11-digit tract + within-tract id
  - published ASPR description: 4-digit suffix
  - observed data accepted by the parser: 4 or 5 digits
- `schoolId`:
  - public school: 11-digit tract + within-tract id
    - published ASPR description: 3-digit suffix
    - observed data accepted by the parser: 3 or 4 digits
  - private school: 5-digit county + `xprvx` + 4-digit within-county id
- `workplaceId`: 11-digit tract + 5-digit within-tract id

These values are parsed into `ixa-fips::FIPSCode` values, and the model's `Community` setting is derived from the tract-level prefix of `homeId`.
