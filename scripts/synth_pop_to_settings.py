import polars as pl
df = (pl.read_csv('input/people_test.csv')
    .with_columns(pl.all().cast(pl.Utf8), pl.col('homeId').str.slice(0, 11).alias('censustractId'))
    .drop('age')
    .unpivot(variable_name='type', value_name='Id')
    .unique()
    .with_columns(pl.col('Id').cast(pl.Int64))
    .write_csv('input/people_test_settings.csv', has_header=True))

