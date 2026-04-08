import os
from pathlib import Path

import matplotlib.pyplot as plt
import polars as pl
import seaborn as sns
from calibrationtools import SamplerReporter

directory_name = "output_indiana"
projections_path = Path(
    "experiments", "phase1", "projection", directory_name, "simulations"
)
figures_path = Path(
    "experiments", "phase1", "projection", directory_name, "figures"
)
os.makedirs(figures_path, exist_ok = True)
show_plots = False

# Reading in the projection data ------------------------------------------------------
aggregated_deaths_report_list = []
imported_cases_timeseries_list = []
prevalence_report_list = []
reporter = SamplerReporter(verbose=True)

with reporter.create_task_progress() as progress:
    handle = reporter.start_task(
        description="Reading projection results... ",
        progress=progress,
        total=len(os.listdir(projections_path)),
    )
    for particle_dir in projections_path.iterdir():
        if particle_dir.is_dir():
            imported_cases_timeseries = pl.read_csv(
                particle_dir / "imported_cases_timeseries.csv"
            )
            aggregated_deaths_report = pl.read_csv(
                particle_dir / "aggregated_deaths_report.csv"
            )
            imported_cases_timeseries = imported_cases_timeseries.with_columns(
                pl.col("imported_infections")
                .cum_sum()
                .over(order_by="time")
                .alias("cumulative_imported_infections"),
                pl.lit(particle_dir.name).alias("seed"),
            )
            aggregated_deaths_report = aggregated_deaths_report.with_columns(
                pl.col("count")
                .cum_sum()
                .over(order_by="t_upper")
                .alias("cumulative_count"),
                pl.lit(particle_dir.name).alias("seed"),
            )
            prevalence_report = (
                pl.read_csv(particle_dir / "person_property_count.csv")
                .with_columns(
                    pl.when(pl.col("age") < 18)
                    .then(pl.lit("Age0To17"))
                    .when((pl.col("age") >= 18) & (pl.col("age") < 50))
                    .then(pl.lit("Age18to49"))
                    .when((pl.col("age") >= 50) & (pl.col("age") < 65))
                    .then(pl.lit("Age50to64"))
                    .otherwise(pl.lit("Age65Plus"))
                    .alias("age_group")
                )
                .group_by(
                    "t", "age_group", "symptom_status", "infection_status"
                )
                .agg(pl.sum("count").alias("count"))
                .sort(["t", "age_group", "symptom_status", "infection_status"])
                .with_columns(pl.lit(particle_dir.name).alias("seed"))
            )
            imported_cases_timeseries_list.append(imported_cases_timeseries)
            aggregated_deaths_report_list.append(aggregated_deaths_report)
            prevalence_report_list.append(prevalence_report)
            reporter.advance(handle)


death_data = pl.concat(aggregated_deaths_report_list)
imported_data = pl.concat(imported_cases_timeseries_list)
prevalence_data = pl.concat(prevalence_report_list)

# plotting ------------------------------------------------------------------------------

# Imported infections over time----------------------
sns.scatterplot(
    data=imported_data.filter(pl.col("imported_infections") > 0),
    x="time",
    y="cumulative_imported_infections",
    alpha=0.2,
)
sns.lineplot(
    data=imported_data,
    x="time",
    y="cumulative_imported_infections",
    estimator="median",
)
plt.xlabel("Time")
plt.ylabel("Cumulative Imported Infections")
plt.savefig(figures_path / "imported_infections_over_time.png", dpi=300)
if show_plots:
    plt.show()
plt.close()

# SIR transmission dynamics----------------------------

# Short time horizon
g = sns.relplot(
    data=prevalence_data.group_by("t", "infection_status", "seed")
    .agg(pl.sum("count").alias("count"))
    .filter(
        (pl.col("t") < 80)
        & (pl.col("infection_status").is_in(["Infectious", "Recovered"]))
    ),
    x="t",
    y="count",
    kind="line",
    hue="infection_status",
    alpha=0.3,
    units="seed",
    estimator=None,
    col="infection_status",
    col_order=["Infectious", "Recovered"],
    # estimator='median',
    # errorbar=lambda x: (x.quantile(0.1), x.quantile(0.9))
)
g.set(ylim=(0, 100))
plt.xlabel("Time")
for ax in g.axes.flat:
    # ax.axvline(x=65, color="red", linestyle="--", label="First case reported (March 6, 2020)")
    ax.axvline(
        x=75,
        color="black",
        linestyle="--",
        label="First death reported (March 16, 2020)",
    )
plt.ylabel("Count")
plt.savefig(figures_path / "infection_status_by_time.png", dpi=300)
if show_plots:
    plt.show()
plt.close()

# Long time horizon
g = sns.relplot(
    data=prevalence_data.group_by("t", "infection_status", "seed").agg(
        pl.sum("count").alias("count")
    ),
    x="t",
    y="count",
    kind="line",
    hue="infection_status",
    alpha=0.3,
    units="seed",
    estimator=None,
    col="infection_status",
    col_order=["Susceptible", "Infectious", "Recovered"],
    # estimator='median',
    # errorbar=lambda x: (x.quantile(0.1), x.quantile(0.9))
)
plt.xlabel("Time")
for ax in g.axes.flat:
    # ax.axvline(x=65, color="red", linestyle="--", label="First case reported (March 6, 2020)")
    ax.axvline(
        x=75,
        color="black",
        linestyle="--",
        label="First death reported (March 16, 2020)",
    )
plt.ylabel("Count")
plt.savefig(figures_path / "infection_status_by_time_full.png", dpi=300)
if show_plots:
    plt.show()
plt.close()

# Deaths over time----------------------
sns.lineplot(
    data=death_data,
    x="t_upper",
    y="cumulative_count",
    alpha=0.2,
    units="seed",
    estimator=None,
)
plt.xlabel("Time")
plt.ylabel("Cumulative Deaths")
plt.yscale("log")
plt.axvline(
    x=75,
    color="black",
    linestyle="--",
    label="First death reported (March 16, 2020)",
)
plt.savefig(figures_path / "cumulative_deaths_over_time.png", dpi=300)
if show_plots:
    plt.show()
plt.close()

# Prevalence by age and symptom status----------------------------
g = sns.relplot(
    data=prevalence_data.filter(
        ~pl.col("symptom_status").is_in(["NoSymptoms", "Dead", "Resolved"])
    )
    .group_by("t", "age_group", "symptom_status", "seed")
    .agg(pl.sum("count").alias("count")),
    x="t",
    y="count",
    kind="line",
    col="age_group",
    row="symptom_status",
    col_order=["Age0To17", "Age18to49", "Age50to64", "Age65Plus"],
    row_order=["Mild", "Severe", "Critical"],
    facet_kws={"sharey": "row", "sharex": True},
    hue="symptom_status",
    alpha=0.2,
    units="seed",
    estimator=None,
    # estimator='median',
    # errorbar = lambda x: (x.quantile(0.025), x.quantile(0.975))
)
g.figure.subplots_adjust(top=0.85)
g.set_axis_labels("Time (days since simulation start)", "Number of people")
g.set_titles("")
plt.savefig(figures_path / "prevalence_by_age_and_symptom_status.png", dpi=300)
if show_plots:
    plt.show()
plt.close()


n_unique_seeds = prevalence_data.select(pl.col("seed").n_unique()).item()

age_group_values = (
    prevalence_data.filter(
        pl.col("symptom_status").is_in(["Severe", "Critical"])
    )
    .group_by("t", "age_group", "symptom_status", "seed")
    .agg(pl.sum("count").alias("count"))
    .filter(pl.col("t") < 85)
    .group_by("age_group", "symptom_status", "t")
    .agg(
        (pl.len() / n_unique_seeds).alias(
            "proportion_of_simualtions_have_had_cases"
        )
    )
)

g = sns.relplot(
    data=age_group_values,
    x="t",
    y="proportion_of_simualtions_have_had_cases",
    kind="line",
    col="age_group",
    col_order=["Age0To17", "Age18to49", "Age50to64", "Age65Plus"],
    col_wrap=2,
    hue="symptom_status",
)

g.set(ylim=(0, 1))

g.figure.suptitle(
    "Observing symptomatic cases in simulations",
    fontsize="x-large",
    fontweight="bold",
)
g.figure.subplots_adjust(top=0.85)
g.set_axis_labels(
    "Time (days since simulation start)",
    "Proportion of simulations with at least one case",
)
for ax in g.axes.flat:
    ax.axvline(
        x=65,
        color="red",
        linestyle="--",
        label="First case reported (March 6, 2020)",
    )
    ax.axvline(
        x=75,
        color="black",
        linestyle="--",
        label="First death reported (March 16, 2020)",
    )
plt.savefig(figures_path / "prevalence_by_age_and_symptom_status.png", dpi=300)
if show_plots:
    plt.show()
plt.close()
