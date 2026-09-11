import json
import random
import subprocess

import numpy as np
import polars as pl
import seaborn as sns
from matplotlib import pyplot as plt

scenarios = [
    "No School Closures;\nNo SIP",
    "School Closures;\nNo SIP",
    "No School Closures;\nSIP",
    "School Closures;\nSIP",
]

hospitalizations = []
attack_rate = []
deaths = []
seeds = [random.randint(1, 10_000) for _ in range(10)]
for i in range(1, 5):
    for seed in seeds:
        input_path = f"experiments/scenarios/input_{i}.json"
        with open(input_path, "r", encoding="utf-8") as f:
            input_data = json.load(f)
            input_data["epimodel.GlobalParams"]["seed"] = 1234 * seed
            with open(input_path, "w", encoding="utf-8") as out_f:
                json.dump(input_data, out_f, indent=2)

        subprocess.run(
            [
                "target/release/ixa-epi-covid",
                "-c",
                input_path,
                "-o",
                "output",
                "-f",
            ],
        )
        hospitalizations.append(
            pl.read_csv("output/current_hospitalizations_report.csv")
            .with_columns(pl.lit(scenarios[i - 1]).alias("scenario"))
            .with_columns(pl.lit(seed).alias("seed"))
        )
        attack_rate.append(
            pl.read_csv("output/attack_rate_report.csv")
            .with_columns(pl.lit(scenarios[i - 1]).alias("scenario"))
            .with_columns(pl.lit(seed).alias("seed"))
        )
        deaths.append(
            pl.read_csv("output/aggregated_deaths_report.csv")
            .with_columns(pl.lit(scenarios[i - 1]).alias("scenario"))
            .with_columns(pl.lit(seed).alias("seed"))
        )

df_hospitalizations = pl.concat(hospitalizations)
df_attack_rate = pl.concat(attack_rate)
df_deaths = pl.concat(deaths)

scenario_order = scenarios


def summarize_over_seeds(df, group_columns, value_column):
    """Calculate mean, 10th percentile, and 90th percentile over seeds."""
    return (
        df.to_pandas()
        .groupby(group_columns, as_index=False)[value_column]
        .agg(
            mean="mean",
            p10=lambda x: x.quantile(0.10),
            p90=lambda x: x.quantile(0.90),
        )
    )


def plot_mean_with_percentile_band(
    summary,
    x_column,
    y_label,
    x_label,
    title=None,
    facet_column=None,
):
    if facet_column is None:
        fig, ax = plt.subplots(figsize=(10, 6))
        axes = [(None, ax)]
    else:
        facet_values = summary[facet_column].unique()
        fig, axes_array = plt.subplots(
            1,
            len(facet_values),
            figsize=(5 * len(facet_values), 4),
            sharex=True,
            sharey=True,
            squeeze=False,
        )
        axes = list(zip(facet_values, axes_array.flat))

    colors = dict(
        zip(
            scenario_order,
            sns.color_palette("tab10", n_colors=len(scenario_order)),
        )
    )

    for facet_value, ax in axes:
        plot_data = summary

        if facet_column is not None:
            plot_data = summary[summary[facet_column] == facet_value]
            ax.set_title(str(facet_value))

        for scenario in scenario_order:
            scenario_data = plot_data[
                plot_data["scenario"] == scenario
            ].sort_values(x_column)

            if scenario_data.empty:
                continue

            x = scenario_data[x_column].to_numpy()
            mean = scenario_data["mean"].to_numpy()
            p10 = scenario_data["p10"].to_numpy()
            p90 = scenario_data["p90"].to_numpy()

            ax.plot(
                x,
                mean,
                label=scenario.replace("\n", " "),
                color=colors[scenario],
            )
            ax.fill_between(
                x,
                p10,
                p90,
                color=colors[scenario],
                alpha=0.20,
            )

        ax.set_xlabel(x_label)
        ax.set_ylabel(y_label)
        ax.grid(alpha=0.25)

    handles, labels = axes[0][1].get_legend_handles_labels()
    fig.legend(handles, labels, loc="lower center", ncol=4)
    if title:
        fig.suptitle(title)

    plt.tight_layout()
    plt.show()


# Hospitalizations: statistics over seeds
hospitalization_summary = summarize_over_seeds(
    df_hospitalizations,
    group_columns=["scenario", "seed", "t"],
    value_column="mean_count",
)

# Aggregate the seed-level values again, excluding seed from the grouping.
hospitalization_summary = hospitalization_summary.groupby(
    ["scenario", "t"], as_index=False
).agg(
    mean=("mean", "mean"),
    p10=("mean", lambda x: x.quantile(0.10)),
    p90=("mean", lambda x: x.quantile(0.90)),
)

plot_mean_with_percentile_band(
    hospitalization_summary,
    x_column="t",
    y_label="Current Hospitalizations",
    x_label="Day",
    title="Current Hospitalizations",
)


# Deaths: statistics over seeds
death_summary = summarize_over_seeds(
    df_deaths,
    group_columns=["scenario", "seed", "t_upper"],
    value_column="count",
)

death_summary = death_summary.groupby(
    ["scenario", "t_upper"], as_index=False
).agg(
    mean=("mean", "mean"),
    p10=("mean", lambda x: x.quantile(0.10)),
    p90=("mean", lambda x: x.quantile(0.90)),
)

plot_mean_with_percentile_band(
    death_summary,
    x_column="t_upper",
    y_label="Incident Deaths",
    x_label="Day",
    title="Incident Deaths",
)


# Attack rate: select the final value for each scenario, age group, and seed
attack_rate_pd = df_attack_rate.to_pandas()

final_attack_rate = attack_rate_pd.loc[
    attack_rate_pd["t_upper"].eq(
        attack_rate_pd.groupby(["scenario", "age_group", "seed"])[
            "t_upper"
        ].transform("max")
    )
].reset_index(drop=True)
# Calculate statistics across seeds
attack_summary = (
    final_attack_rate.groupby(["scenario", "age_group"])["attack_rate"]
    .agg(
        mean="mean",
        p10=lambda x: x.quantile(0.10),
        p90=lambda x: x.quantile(0.90),
    )
    .reset_index()
)

age_groups = list(attack_summary["age_group"].unique())
scenario_order = scenarios

fig, ax = plt.subplots(figsize=(12, 7))

x = np.arange(len(age_groups))
bar_width = 0.8 / len(scenario_order)

for i, scenario in enumerate(scenario_order):
    plot_data = (
        attack_summary[attack_summary["scenario"] == scenario]
        .set_index("age_group")
        .reindex(age_groups)
    )

    positions = x + (i - (len(scenario_order) - 1) / 2) * bar_width

    # Asymmetric errors: mean-to-10th and mean-to-90th percentile
    error_bars = np.vstack(
        [
            plot_data["mean"] - plot_data["p10"],
            plot_data["p90"] - plot_data["mean"],
        ]
    )

    ax.bar(
        positions,
        plot_data["mean"],
        width=bar_width,
        yerr=error_bars,
        capsize=4,
        label=scenario.replace("\n", " "),
    )

ax.set_xticks(x)
ax.set_xticklabels(age_groups)
ax.set_xlabel("Age Group")
ax.set_ylabel("Final Attack Rate")
ax.set_title("Final Attack Rate by Age Group")
ax.legend(title="Scenario", bbox_to_anchor=(1.02, 1), loc="upper left")
ax.grid(axis="y", alpha=0.25)

plt.tight_layout()
plt.show()
