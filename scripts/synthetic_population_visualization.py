import polars as pl 
import seaborn
import matplotlib.pyplot as plt
import pandas as pd
import numpy as np
from shapely.geometry import LineString, Point
from tqdm import tqdm
import geopandas as gpd
import pygris

import requests
from io import BytesIO
# USDA RUCA data source (latest public release)
RUCA_PATH_2020 = "/home/xui6/Downloads/RUCA-codes-2020-tract.csv"
RUCA_PATH_2010 = "/home/xui6/Downloads/ruca2010revised.csv"


def load_ruca_data():
    df = pd.read_csv(RUCA_PATH_2010, encoding="latin-1", dtype={"State-County-Tract FIPS Code": str})
    df = df[df["StateFIPS20"] == "18"]
    df = df.rename(columns={"State-County-Tract FIPS Code": "GEOID"})
    return df

def get_ruca_by_tract(tract_fips, ruca_df):
    """
    Retrieves RUCA code(s) for a given Census tract FIPS code.
    Args:
        tract_fips (str): 11-digit Census tract FIPS code.
        ruca_df (pd.DataFrame): RUCA DataFrame.
    Returns:
        pd.DataFrame: Matching RUCA record(s).
    """
    tract_fips = tract_fips.strip()
    result = ruca_df[ruca_df["tract_fips"] == tract_fips]
    if result.empty:
        print(f"No RUCA data found for tract {tract_fips}")
    return result

def load_synth_pop(synth_pop_file):
    df = (
        pl.read_csv(synth_pop_file)
        .with_columns(
                pl.all().cast(pl.Utf8),
                pl.col("homeId").cast(pl.Utf8).str.slice(0, 11).alias("Community"),
        )
        .with_row_index("person_id")
        .unpivot(index=["person_id", "age"], variable_name="setting_category", value_name="setting_code")
        .drop_nulls()
        .with_columns(
                pl.col("setting_category")
                .str.to_titlecase()
                .str.replace("id$", "")
        )
        .with_columns(
            pl.col("setting_code").str.slice(0, 11).alias("GEOID")
        )
    )

    # # Load RUCA data
    # ruca_df = load_ruca_data()
    # print(df)
    # print(ruca_df)
    # # Add RUCA code to the DataFrame
    # df = df.join(
    #     pl.DataFrame(ruca_df[["GEOID", "PrimaryRUCA","PrimaryRUCADescription","PopDensity"]]),
    #     on="GEOID",
    #     how="left"
    # )

    return df

def plot_group_size_distribution(synth_pop):
    group_size_dist = (
        synth_pop.group_by(["setting_category", "setting_code"])
        .agg(pl.count("person_id").alias("group_size"))
    )
    g = seaborn.FacetGrid(group_size_dist.to_pandas(), col="setting_category", sharex=False, sharey=False)
    g.map(seaborn.histplot, "group_size", bins=45)  # Increased bin size
    g.set_axis_labels("Group Size", "Count")
    for ax in g.axes_dict.values():
        ax.set_title(ax.get_title().replace("setting_category = ", ""))
    plt.show()

def plot_contact_matrix(synth_pop, setting_category_string, sample_size):
    unique_settings = synth_pop.filter(pl.col("setting_category") == setting_category_string)["setting_code"].unique()
    setting_contact = np.zeros((100, 100), dtype=float)
    synth_pop = synth_pop.filter(pl.col("setting_category") == setting_category_string)
    # Randomly sample 100 unique homes   

    sampled_settings = np.random.choice(unique_settings, size=min(sample_size, len(unique_settings)), replace=False)
    synth_pop = synth_pop.filter(pl.col("setting_code").is_in(sampled_settings))

    # Add total number of people with each age as a column
    age_counts = synth_pop.group_by("age").agg(pl.count("person_id").alias("age_count"))
    synth_pop = synth_pop.join(age_counts, on="age")
    for row in tqdm(synth_pop.iter_rows(named=True)):
        person_id = row["person_id"]
        age = int(row["age"])
        total_age_count = row["age_count"]
        category = row["setting_category"]
        code = row["setting_code"]
        setting_pop = synth_pop.filter((pl.col("setting_category") == category) & (pl.col("setting_code") == code))
        setting_pop = setting_pop.filter(pl.col("person_id") != person_id)
        ages = setting_pop["age"].to_list()
        for other_age in ages:
            setting_contact[age, int(other_age)] += 1.0 / float(total_age_count)

    plt.figure(figsize=(10, 8))
    seaborn.heatmap(setting_contact[::-1], cmap="rocket", cbar=True)
    plt.title("Average Number of " + setting_category_string + " Contacts By Age")
    plt.xlabel("Age")
    plt.ylabel("Age")
    plt.xticks(ticks=np.arange(0, 100, 5), labels=np.arange(0, 100, 5))
    plt.yticks(ticks=np.arange(0, 100, 5), labels=np.arange(100, 0, -5))
    plt.show()
    

def plot_distribution_of_age_by_work(synth_pop, setting_category_string):
    setting_pop = synth_pop.filter(pl.col("setting_category") == setting_category_string)
    setting_pop = setting_pop.with_columns(pl.col("age").cast(pl.Int32))
    setting_pop = setting_pop.with_columns(pl.col("age").cast(pl.Float64).mean().over("setting_code").alias("average_age"))
    setting_pop = setting_pop.with_columns(pl.col("age").cast(pl.Float64).max().over("setting_code").alias("max_age"))
    setting_pop = setting_pop.with_columns(
        pl.col("person_id").count().over("setting_code").alias("total_count")
    )

    setting_pop_drop = setting_pop.unique(subset="setting_code")
    plt.figure(figsize=(10, 6))
    seaborn.scatterplot(setting_pop_drop.to_pandas(), x="total_count", y="average_age")
    plt.title("Workplace size vs average age n = " + str(len(setting_pop_drop)) + " workplaces")
    plt.xlabel("Workplace size")
    plt.ylabel("Average Age")
    plt.show()

    # Filter out settings with no age data
    # setting_pop = setting_pop.sort("age")
    plt.figure(figsize=(10, 6))
    seaborn.histplot(setting_pop.to_pandas(), x="age", bins=50, kde=False)
    plt.title("Distribution of Age in " + setting_category_string)
    plt.xlabel("Age")
    plt.ylabel("Count")
    plt.show()

    unique_setting_codes = setting_pop["setting_code"].unique()[:20]
    setting_pop = setting_pop.filter(pl.col("setting_code").is_in(unique_setting_codes))
    g = seaborn.FacetGrid(setting_pop.to_pandas(), col="setting_code", col_wrap=4, sharex=False, sharey=False)
    g.map(seaborn.histplot, "age", bins=30)
    g.set_axis_labels("Age", "Count")
    g.set_titles("{col_name}")
    plt.show()

def plot_distribution_of_age_by_school(synth_pop, setting_category_string):
    setting_pop = synth_pop.filter(pl.col("setting_category") == setting_category_string)
    setting_pop = setting_pop.with_columns(pl.col("age").cast(pl.Int32))
    setting_pop = setting_pop.with_columns(pl.col("age").cast(pl.Float64).mean().over("setting_code").alias("average_age"))
    setting_pop = setting_pop.with_columns(pl.col("age").cast(pl.Float64).max().over("setting_code").alias("max_age"))
    setting_pop = setting_pop.with_columns(
        pl.col("age").filter(pl.col("age") > 21).count().over("setting_code").alias("count_over_21")
    )
    setting_pop = setting_pop.with_columns(
        pl.col("age").filter(pl.col("age") < 21).count().over("setting_code").alias("count_under_21")
    )
    setting_pop = setting_pop.with_columns(
        pl.col("person_id").count().over("setting_code").alias("total_count")
    )



    setting_pop_drop = setting_pop.unique(subset="setting_code")
    setting_pop_drop = setting_pop_drop.with_columns(
        (pl.col("count_under_21") / pl.col("count_over_21")).fill_nan(0).alias("student_teacher_ratio")
    )

    print(setting_pop_drop.select(["setting_code", "average_age", "total_count", "count_under_21", "count_over_21", "student_teacher_ratio"]))
    no_teachers = setting_pop_drop.filter(pl.col("student_teacher_ratio") == np.inf)
    teachers = setting_pop_drop.filter(pl.col("student_teacher_ratio") != np.inf)

    plt.figure(figsize=(10, 6))
    seaborn.scatterplot(teachers.to_pandas(), x="total_count", y="average_age", hue = "student_teacher_ratio", palette="viridis", legend= True)
    plt.title("School size vs average age by student teacher ratio n = " + str(len(teachers)) + " of " + str(len(setting_pop_drop)) + " schools")
    plt.xlabel("School size")
    plt.ylabel("Average Age")
    plt.legend(title="Students per Teacher")
    plt.show()

    plt.figure(figsize=(10, 6))
    seaborn.scatterplot(no_teachers.to_pandas(), x="total_count", y="average_age")
    plt.title("School size vs average age with no teachers n = " + str(len(no_teachers)) + " of " + str(len(setting_pop_drop)) + " schools")
    plt.xlabel("School size")
    plt.ylabel("Average Age")
    plt.legend(title="Students per Teacher")
    plt.show()
    
    # Select 5 unique setting code values
    
    # Filter out settings with no age data
    # setting_pop = setting_pop.sort("age")
    plt.figure(figsize=(10, 6))
    seaborn.histplot(setting_pop.to_pandas(), x="age", bins=50, kde=False)
    plt.title("Distribution of Age in " + setting_category_string)
    plt.xlabel("Age")
    plt.ylabel("Count")
    plt.show()

    unique_setting_codes = setting_pop["setting_code"].unique()[:20]
    setting_pop = setting_pop.filter(pl.col("setting_code").is_in(unique_setting_codes))
    g = seaborn.FacetGrid(setting_pop.to_pandas(), col="setting_code", col_wrap=4, sharex=False, sharey=False)
    g.map(seaborn.histplot, "age", bins=30)
    g.set_axis_labels("Age", "Count")
    g.set_titles("{col_name}")
    plt.show()

def plot_map_of_indiana(df):    
    # Filter for Indiana census tracts
    indiana_tracts = pygris.tracts(state="18", year=2016)
    # Join indiana_tracts and df on "GEOID"
    indiana_tracts = indiana_tracts.merge(df.to_pandas(), left_on="GEOID", right_on="GEOID", how="left")
    # Calculate average age by GEOID
    indiana_tracts['age'] = indiana_tracts['age'].astype(float)
    indiana_tracts['average_age'] = indiana_tracts.groupby(['GEOID', 'setting_category'])['age'].transform('mean')
    indiana_tracts['population_size'] = indiana_tracts.groupby(['GEOID', 'setting_category'])['person_id'].transform('count')
    indiana_tracts['number_of_groups'] = indiana_tracts.groupby(['GEOID', 'setting_category'])['setting_code'].transform('nunique')
    indiana_tracts["group_size"] = indiana_tracts.groupby(['setting_category','setting_code'])['person_id'].transform('count')
    indiana_tracts['average_group_size'] = indiana_tracts.groupby(['GEOID', 'setting_category'])["group_size"].transform('mean')
    # Remove rows with duplicate values of GEOID
    indiana_homes = indiana_tracts[indiana_tracts['setting_category'] == 'Home']
    indiana_schools = indiana_tracts[indiana_tracts['setting_category'] == 'School']
    indiana_work = indiana_tracts[indiana_tracts['setting_category'] == 'Workplace']
    indiana_community = indiana_tracts[indiana_tracts['setting_category'] == 'Community']
    indiana_tracts = indiana_tracts.drop_duplicates(subset="GEOID")
    indiana_homes = indiana_homes.drop_duplicates(subset="GEOID")
    indiana_schools = indiana_schools.drop_duplicates(subset="GEOID")
    indiana_work = indiana_work.drop_duplicates(subset="GEOID")
    indiana_community = indiana_community.drop_duplicates(subset="GEOID")
    # Plot Indiana counties
    fig, axes = plt.subplots(2, 4, figsize=(40, 20), constrained_layout=True)

    indiana_homes.plot(column="average_age", cmap="viridis", edgecolor='black', linewidth=0.1, ax=axes[0, 0], legend=True, label="Average Age")
    axes[0, 0].set_title("Average Age in Census Tract")
    axes[0, 0].legend(fontsize=20)
    axes[0, 0].axis('off')
    


    indiana_homes.plot(column="population_size", cmap="viridis", edgecolor='black', linewidth=0.1, ax=axes[1, 0], legend=True)
    axes[1, 0].set_title("Population Size in Census Tract")
    axes[1, 0].axis('off')


    indiana_schools.plot(column="number_of_groups", cmap="viridis", edgecolor='black', linewidth=0.1, ax=axes[0, 1], legend=True)
    axes[0, 1].set_title("Number of Schools")
    axes[0, 1].axis('off')


    indiana_work.plot(column="number_of_groups", cmap="viridis", edgecolor='black', linewidth=0.1, ax=axes[0, 2], legend=True)
    axes[0, 2].set_title("Number of Workplaces")
    axes[0, 2].axis('off')


    indiana_homes.plot(column="number_of_groups", cmap="viridis", edgecolor='black', linewidth=0.1, ax=axes[0, 3], legend=True)
    axes[0, 3].set_title("Number of Homes")
    axes[0, 3].axis('off')


    indiana_homes.plot(column="average_group_size", cmap="viridis", edgecolor='black', linewidth=0.1, ax=axes[1, 3], legend=True)
    axes[1, 3].set_title("Average Home Size")
    axes[1, 3].axis('off')

    ruca = indiana_tracts.drop_duplicates(subset="GEOID").plot(column="PrimaryRUCA", cmap="viridis", edgecolor='black', linewidth=0.1, ax=axes[0, 4], legend=True)
    axes[1, 1].set_title("RUCA Codes")
    axes[1, 1].axis('off')


    plt.suptitle("Indiana Census Tract Visualization", fontsize=30)
    for ax in axes.flat:
        ax.title.set_fontsize(30)  # Set title font size
    plt.savefig("output/indiana_censustract_visualization.png", dpi=600)
    plt.close()


def plot_workplace_communiting_map(synth_pop_file):
   # Filter for Indiana census tracts
    indiana_tracts = pygris.counties(state="18", year=2016)

    
    df = (
        pl.read_csv(synth_pop_file)
        .with_row_index("person_id")
        .filter(pl.col("schoolId").is_not_null())
        .with_columns(
                pl.all().cast(pl.Utf8),
                pl.col("homeId").cast(pl.Utf8).str.slice(0, 5).alias("HomeTract"),
        )
        .with_columns(
                pl.all().cast(pl.Utf8),
                pl.col("schoolId").cast(pl.Utf8).str.slice(0, 5).alias("WorkTract"),
        )
        .drop(["age", "schoolId", "workplaceId", "homeId"])
    )
    data = df.to_pandas().merge(indiana_tracts, left_on="HomeTract", right_on="GEOID", how="left")
    data = data.merge(indiana_tracts, left_on="WorkTract", right_on="GEOID", how="left", suffixes=("_home", "_work"))
    print(data)

    # Create a GeoDataFrame for flow lines
    flow_lines = gpd.GeoDataFrame(columns=["geometry", "count"], crs=indiana_tracts.crs)
    print(flow_lines)
    # Group by HomeTract and WorkTract to count flows
    flow_counts = data.groupby(["HomeTract", "WorkTract"]).size().reset_index(name="count")
    
    
    # flow_counts = pd.DataFrame(flow_counts)
    print(flow_counts)
    between_county_flows = flow_counts[flow_counts["HomeTract"] != flow_counts["WorkTract"]]
    intra_county_flows = flow_counts[flow_counts["HomeTract"] == flow_counts["WorkTract"]]
    # Create flow lines for the first 10 rows
    for _, row in between_county_flows.iterrows():
        home_geom = indiana_tracts[indiana_tracts["GEOID"] == row["HomeTract"]].geometry.values[0] if not indiana_tracts[indiana_tracts["GEOID"] == row["HomeTract"]].empty else None
        work_geom = indiana_tracts[indiana_tracts["GEOID"] == row["WorkTract"]].geometry.values[0] if not indiana_tracts[indiana_tracts["GEOID"] == row["WorkTract"]].empty else None
        if home_geom is not None and work_geom is not None:
            line = LineString([home_geom.centroid, work_geom.centroid])
            flow_lines = pd.concat([flow_lines, gpd.GeoDataFrame([{"geometry": line, "count": row["count"]}], crs=flow_lines.crs)], ignore_index=True)
    # Plot the map
    fig, ax = plt.subplots(1, 1, figsize=(15, 15))
    indiana_tracts.boundary.plot(ax=ax, linewidth=0.5, color="black")
    map = flow_lines.plot(ax=ax, column="count", linewidth=flow_lines["count"] / flow_lines["count"].max() * 20, cmap="Blues", legend=False)
    plt.title("Flow Between Home County and School County in Indiana")
    plt.axis("off")
    sm = plt.cm.ScalarMappable(cmap="Blues", norm=plt.Normalize(vmin=flow_lines["count"].min(), vmax=flow_lines["count"].max()))
    sm._A = []  # Add this line to avoid errors with ScalarMappable
    plt.colorbar(sm, ax=ax, label="Number of Commuters")
    plt.show()

    plt.figure(figsize=(10, 8))
    indiana_tracts = indiana_tracts.merge(intra_county_flows, left_on="GEOID", right_on="HomeTract", how="left")
    indiana_tracts.plot(column="count", cmap="Blues", edgecolor='black', linewidth=0.1, legend=True)
    plt.show()


df = load_synth_pop("input/in.csv")
# plot_workplace_communiting_map("input/in.csv")
# plot_map_of_indiana(df)
# plot_group_size_distribution(df)
# plot_contact_matrix(df, "Home", 10000)
# plot_contact_matrix(df, "Workplace", 3000)
# plot_contact_matrix(df, "School", 200)
# plot_contact_matrix(df, "Community", 3)
plot_distribution_of_age_by_school(df, "School")
# plot_distribution_of_age_by_work(df, "Workplace")