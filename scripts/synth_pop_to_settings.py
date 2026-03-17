import polars as pl

df = (
    pl.read_csv("input/people_test.csv")
    .with_columns(
        pl.all().cast(pl.Utf8),
        pl.col("homeId").cast(pl.Utf8).str.slice(0, 11).alias("censustractId"),
    )
    .drop("age")
    .unpivot(variable_name="setting_category", value_name="setting_code")
    .unique()
)
df.write_csv("input/synth_pop_to_settings.csv")
