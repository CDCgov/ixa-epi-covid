import polars as pl

# Load CSV file using Polars
csv_file_path = "input/in.csv"
dataframe = pl.read_csv(csv_file_path)
# Remove the 'age' column from the dataframe
dataframe = dataframe.drop("age")

# Convert all columns to strings
dataframe = dataframe.with_columns(
    [pl.col(column).cast(pl.Utf8) for column in dataframe.columns]
)

# Create a new column 'censustract' with the first 11 characters of the 'home' column
dataframe = dataframe.with_columns(
    pl.col("homeId").str.slice(0, 11).alias("censustractId")
)

# Melt the specified columns into 'settingcategory' and 'settingcode'
dataframe = dataframe.unpivot(
    on=["homeId", "schoolId", "workplaceId", "censustractId"],
    variable_name="settingcategory",
    value_name="settingcode"
)

# Get unique rows in the dataframe
dataframe = dataframe.unique()


# Display the first few rows of the dataframe
print(dataframe)
# Drop rows with NaN values
dataframe = dataframe.drop_nulls()
print(dataframe)
# Save the dataframe to a CSV file
output_csv_path = "input/in_settings.csv"
dataframe.write_csv(output_csv_path)