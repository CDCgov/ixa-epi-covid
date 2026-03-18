import polars as pl

# Load CSV file using Polars
csv_file_path = "input/in.csv"
people = pl.read_csv(csv_file_path)
# Remove the 'age' column from the dataframe
people = people.drop("age")

# Convert all columns to strings
people = people.with_columns(
    [pl.col(column).cast(pl.Utf8) for column in people.columns]
)

# Create a new column 'censustract' with the first 11 characters of the 'home' column
people = people.with_columns(
    pl.col("homeId").str.slice(0, 11).alias("censustractId")
)

# Melt the specified columns into 'settingcategory' and 'settingcode'
people = people.unpivot(
    on=["homeId", "schoolId", "workplaceId", "censustractId"],
    variable_name="settingcategory",
    value_name="settingcode"
)

# Get unique rows in the dataframe
people = people.unique()


# Display the first few rows of the dataframe
print(people)
# Drop rows with NaN values
people = people.drop_nulls()
print(people)
# Save the dataframe to a CSV file
output_csv_path = "input/in_settings.csv"
people.write_csv(output_csv_path)