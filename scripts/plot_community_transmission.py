from pathlib import Path

import polars as pl 
import seaborn as sns
import matplotlib.pyplot as plt
import pandas as pd
import numpy as np
from shapely.geometry import LineString, Point
from tqdm import tqdm
import geopandas as gpd
import pygris

import requests
from io import BytesIO
from scipy.stats import ks_2samp

## ===============================#
## Setup ---------
## ===============================#

sns.set_style("whitegrid")

## ===============================#
## Read person properties reports
## ===============================#

def plot_community_transmission_map():
   # Filter for Indiana census tracts
    indiana_tracts = pygris.tracts(state="18", year=2016)

    
    transmission_report = pl.read_csv(
        Path("output") / "transmission_report.csv",
        dtypes={col: pl.Utf8 for col in pl.read_csv(Path("output") / "transmission_report.csv", n_rows=0).columns}
    )

    filtered_report = transmission_report.filter(
        pl.col("infector_setting_id") != pl.col("infectee_setting_id")
    )

    filtered_report = filtered_report.rename(
        {"infector_fips": "infector_GEOID", "infectee_fips": "infectee_GEOID"}
    )
    print(filtered_report.shape)
    data = filtered_report.to_pandas().merge(indiana_tracts, left_on="infector_GEOID", right_on="GEOID", how="left")
    data = data.merge(indiana_tracts, left_on="infectee_GEOID", right_on="GEOID", how="left", suffixes=("_infector", "_infectee"))
    print(data)

    # Create a GeoDataFrame for flow lines
    flow_lines = gpd.GeoDataFrame(columns=["geometry", "count"], crs=indiana_tracts.crs)
    print(flow_lines)
    # Group by HomeTract and WorkTract to count flows
    flow_counts = data.groupby(["infector_GEOID", "infectee_GEOID"]).size().reset_index(name="count")
    
    
    # flow_counts = pd.DataFrame(flow_counts)
    print(flow_counts)
    between_county_flows = flow_counts[flow_counts["infector_GEOID"] != flow_counts["infectee_GEOID"]]
    # intra_county_flows = flow_counts[flow_counts["infector_GEOID"] == flow_counts["infectee_GEOID"]]
    # Create flow lines for the first 10 rows
    for _, row in tqdm(between_county_flows.iterrows()):
        home_geom = indiana_tracts[indiana_tracts["GEOID"] == row["infector_GEOID"]].geometry.values[0] if not indiana_tracts[indiana_tracts["GEOID"] == row["infector_GEOID"]].empty else None
        work_geom = indiana_tracts[indiana_tracts["GEOID"] == row["infectee_GEOID"]].geometry.values[0] if not indiana_tracts[indiana_tracts["GEOID"] == row["infectee_GEOID"]].empty else None
        if home_geom is not None and work_geom is not None:
            line = LineString([home_geom.centroid, work_geom.centroid])
            flow_lines = pd.concat([flow_lines, gpd.GeoDataFrame([{"geometry": line, "count": row["count"]}], crs=flow_lines.crs)], ignore_index=True)
    # Plot the map
    fig, ax = plt.subplots(1, 1, figsize=(15, 15))
    indiana_tracts.boundary.plot(ax=ax, linewidth=0.5, color="black")
    map = flow_lines.plot(ax=ax, column="count", linewidth=flow_lines["count"] / flow_lines["count"].max() * 20, cmap="viridis", legend=False)
    plt.title("Infections Between Different Census Tracts in Indiana due to Radiation Model")
    plt.axis("off")
    sm = plt.cm.ScalarMappable(cmap="viridis", norm=plt.Normalize(vmin=flow_lines["count"].min(), vmax=flow_lines["count"].max()))
    sm._A = []  # Add this line to avoid errors with ScalarMappable
    plt.colorbar(sm, ax=ax, label="Number of Infections")
    plt.show()

    # plt.figure(figsize=(10, 8))
    # indiana_tracts = indiana_tracts.merge(intra_county_flows, left_on="GEOID", right_on="infector_GEOID", how="left")
    # indiana_tracts.plot(column="count", cmap="Blues", edgecolor='black', linewidth=0.1, legend=True)
    # plt.show()

plot_community_transmission_map()