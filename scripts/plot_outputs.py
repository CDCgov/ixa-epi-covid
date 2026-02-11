import polars as pl
import seaborn as sns
from pathlib import Path
import matplotlib.pyplot as plt

## ===============================#
## Setup ---------
## ===============================#

sns.set_style("whitegrid")

## ===============================#
## Read person properties reports
## ===============================#

person_property_report = pl.read_csv(
  Path("output") / "person_property_count.csv"
)

## ===============================#
## Plots
## ===============================#

# Infectious curves
df_plot = (
  person_property_report
  .group_by(["t", "infection_status"])
  .agg(pl.col("count").sum())
)

plt.figure(figsize=(10, 6))
sns.lineplot(
  data=df_plot.to_pandas(),
  x="t",
  y="count",
  hue="infection_status"
)
plt.xlabel("Day")
plt.ylabel("Number of people")
plt.tight_layout()
plt.show()
