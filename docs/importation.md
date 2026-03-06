# Imported infection time series

The methods used to generate the imported data time series can be found in the `packages/importation/README.md` and supporting documentation. Briefly, we use national or state level data to generate a timeseries of imported infection attempts into the model.

Imported infection attempts can occur any time in the model, but will only be successfully included if the individual sampled to be infected as an externally generated infection is Susceptible when the attempt is made. Otherwise, the infection attempt fails, and the time series progresses to the next individual. The data read in to generate infection attempts is structured with `time` and `imported_infections` columns, which specify the time at which imported infection attempts are made and how many to make at that time, respectively.

Much like the seeded infections in the [initialization](initialization.md) of the model, imported cases could occur before time 0.0 in the model. Because the transmission model does not begin until time 0.0, the imported infections that occur in negative time likewise do not contribute to novel, locally derived infections. However, all imported infections begin their infectious period duration at the time of importation, and thus, unlike seeded infections, are not guaranteed to still be infectious by time transmission is activated in the model.
