import subprocess
from matplotlib import pyplot as plt
import polars as pl
import seaborn as sns

scenarios = ["No School Closures;\nNo SIP",
             "School Closures;\nNo SIP",
             "No School Closures;\nSIP",
             "School Closures;\nSIP"]
hospitalizations = []
attack_rate = []
deaths = []
for i in range(1,5):
    subprocess.run(
        ["target/release/ixa-epi-covid", "-c", f"experiments/scenarios/input_{i}.json", "-o", "output", "-f"],
    )
    hospitalizations.append(pl.read_csv("output/current_hospitalizations_report.csv").with_columns(pl.lit(scenarios[i-1]).alias("scenario")))
    attack_rate.append(pl.read_csv("output/attack_rate_report.csv").with_columns(pl.lit(scenarios[i-1]).alias("scenario")))
    deaths.append(pl.read_csv("output/aggregated_deaths_report.csv").with_columns(pl.lit(scenarios[i-1]).alias("scenario")))

df_hospitalizations = pl.concat(hospitalizations)
df_attack_rate = pl.concat(attack_rate)
df_deaths = pl.concat(deaths)

df_deaths.write_csv("output/deaths.csv")
# Deaths curves
df_deaths = df_deaths.group_by(["t_upper", "scenario"]).agg(
    pl.col("count").sum()
)


plt.figure(figsize=(10, 6))
sns.lineplot(
    data=df_deaths.to_pandas(), x="t_upper", y="count", hue="scenario"
)
plt.xlabel("Week")
plt.ylabel("Number of people")
plt.tight_layout()
plt.show()