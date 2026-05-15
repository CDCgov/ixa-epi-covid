import pygris
import pandas as pd
import numpy as np
from scipy.spatial.distance import pdist, squareform
from tqdm import tqdm

def load_synth_pop(synth_pop_file):
    df = pd.read_csv(synth_pop_file)
    df = df.astype(str)
    df["Community"] = df["homeId"].str[:11]
    df = df.reset_index(drop=True).rename_axis('person_id').reset_index()
    df = df.melt(id_vars=["person_id", "age"], var_name="setting_category", value_name="setting_code")
    df = df.dropna()
    df["setting_category"] = df["setting_category"].str.replace(r"id$", "", regex=True).str.title()
    df["GEOID"] = df["setting_code"].str[:11]

    return df

indiana_tracts = pygris.tracts(state="18", year=2016)
print(indiana_tracts.head())
print(indiana_tracts.columns)
# indiana_tracts.to_csv('indiana_tracts.csv', index=False)

population = indiana_tracts['GEOID']

# Calculate distances between all census tracts using INTPTLAT and INTPTLON
indiana_tracts['INTPTLAT'] = indiana_tracts['INTPTLAT'].astype(float)
indiana_tracts['INTPTLON'] = indiana_tracts['INTPTLON'].astype(float)
indiana_tracts = indiana_tracts.set_index('GEOID')

coords = indiana_tracts[['INTPTLAT', 'INTPTLON']].values
distances = squareform(pdist(coords, metric='euclidean'))
distance_df = pd.DataFrame(distances, 
                           index=indiana_tracts.index, 
                           columns=indiana_tracts.index)
distance_df = distance_df.reset_index().melt(id_vars='GEOID', var_name='GEOID_2', value_name='distance')

population = population.to_frame().reset_index(drop=True)
population.columns = ['GEOID']
population['population'] = np.random.randint(100000, 1000001, size=len(population))

# Merge population with distance_df
distance_df = distance_df.merge(population, on='GEOID', how='left').rename(columns={'population': 'population_1'})
distance_df = distance_df.merge(population, left_on='GEOID_2', right_on='GEOID', how='left').rename(columns={'population': 'population_2'}).drop(columns=['GEOID_y'])
distance_df = distance_df.rename(columns={'GEOID_x': 'GEOID_1'})

# Calculate radiation model flow between GEOID_1 and GEOID_2
# Radiation model: T_ij = T_i * (p_i * p_j) / ((p_i + s_ij)(p_i + s_ij + p_j))
# where s_ij is the population in a ring between i and j
distance_df = distance_df.sort_values(['GEOID_1', 'distance']).reset_index(drop=True)

print(distance_df)
# Vectorized population_in_ring calculation
distance_df['population_in_ring'] = distance_df.groupby('GEOID_1').apply(
    lambda x: x['population_2'].shift(1).fillna(0).cumsum()
).reset_index(level=0, drop=True)

# Avoid division by zero
distance_df['denominator'] = (distance_df['population_1'] + distance_df['population_in_ring']) * (distance_df['population_1'] + distance_df['population_in_ring'] + distance_df['population_2'])
distance_df['denominator'] = distance_df['denominator'].replace(0, 1)

# Calculate radiation model flow
distance_df['flow'] = (distance_df['population_1'] * distance_df['population_2']) / distance_df['denominator']

# Drop intermediate columns
distance_df = distance_df.drop(columns=['distance', 'population_1', 'population_2', 'population_in_ring', 'denominator'])
distance_df = distance_df.rename(columns={'GEOID_1': 'from', 'GEOID_2': 'to'})

# Separate GEOID into state, county, and census tract codes
distance_df['from_state'] = distance_df['from'].str[:2]
distance_df['from_county'] = distance_df['from'].str[2:5]
distance_df['from_tract'] = distance_df['from'].str[5:11]

distance_df['to_state'] = distance_df['to'].str[:2]
distance_df['to_county'] = distance_df['to'].str[2:5]
distance_df['to_tract'] = distance_df['to'].str[5:11]

# Save to CSV
distance_df.to_csv('input/distance_df.csv', index=False)

# Save head as dummy file
distance_df.head().to_csv('input/distance_df_dummy.csv', index=False)









