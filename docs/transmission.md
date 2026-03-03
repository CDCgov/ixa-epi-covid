# Overview
The transmission model controls person-to-person transmission between an infectious agent and susceptible agents. Agents become infectious immediately upon successful infection attempt. Once infectious, attempt infections scheduled at a rate determined by their infectiousness rate function and the settings they occupy. Infectious agents continue to schedule infection attempts until the duration of their infectious period is over, at which point they switch to Recovered.

# Transmission model details

## Infection Propagation Loop
1. Watch for an agent becoming infectious (S --> I transition).
2. Schedule an infection attempt accvording to the maximum possible rate determined by  the setting s that the individual occupies
3. Draw a susceptible contact from the agent's contact group for the infection attempt.
    1. First, a setting for the infection attempt is sampled sampled based on the weight of time spent and effective number of contacts.
    2. Second, a contact is sampled uniformly from the setting of the infection attempt

    If the infection attempt is successful, the susceptible contact is labeled as infectious.
4. Repeat steps #2 and #3 regardless of whether the attempt is successful and ending when the infectious duration is over.
6. Label the agent as recovered.

## `Constant` infectiousness rates
A constant infectiousness rate function is defined by a rate parameter and duration parameter. An individual's infectiousness rate does not vary during their infection. Using this approach results in a Poisson process with exponentially distributed time between infection attempts.

## Forecasting Infection Attempts
It is not possible to know how an individual's infectiousness rate function will change over the course of their infection duration. We therefore use a rejection sampling approach in which forecasted infection attempts are generated using the individual's maximum infectiousness rate function. This function is defined as the individual's infectiousness rate function scaled by the largest setting specific transmission rate modifier. At the time of the forecasted infection attempt, the individual's actual infectiousness rate is calculated. The forecast is then evaluated to be successful with probability equivalent to the ratio of the actual and maximum infectiousness at the current time, which is guaranteed to be less than one. If the forecasted infection attempt is successful, the remainder of the infection propagation loop is executed.

Given an individual's maximum infectiousness rate function, the next forecasted infection is stochastically generated using inverse transform sampling. A number of events to occur is sampled from an exponential distribution with rate one. Given the number of events the expected time to for those events to occur is calculated from cumulative growth rate of the maximum infectiousness rate curve at the current time. This time is returned, and the next forecasted infection attempt is scheduled at that time in the future. More information can be found in the [appendix](appendix/time-varying-infectiousness.md)

## Settings, contacts, and transmission
As discussed in [settings documentation](settings.md), settings can reflect density dependent transmission by setting category specific parameters $\alpha$. The rate of infection attempts in a setting therefore scalaes with the number of active individuals in that setting $N$ with the form $(N-1)^\alpha$. For an individual's active itinerary, the weighted average of the density dependent scaling factors is calculated from the proportion of time the individual spends in each setting. The largest setting specific modifier is tracked for each individual and used to calculate the maximum rate of infection attempts, akin to assuming that the maximum rate is that which would occur if an inidivudal spent all of their time in the setting with the highest scaled rate.
