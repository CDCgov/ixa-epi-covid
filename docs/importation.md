# Imported infection time series

The methods used to generate the imported data time series can be found in the `packages/importation/README.md` and supporting documentation. Briefly, we use national or state level data to sample a timeseries of imported infection attempts into the model.

Imported infection attempts can occur any time in the model, but will only be successfullky included if the indiovidual sampled to be infected as an externally generated infection is Susceptible when the attempt is made. Otherwise, the infection attempt fails and the time series progresses to the next individual. The data read in to generate infection attempts is structured with `time` and `imported_infections` columns, which specify the time at which imported infection attempts are made and how many to make at that time, respectively.
