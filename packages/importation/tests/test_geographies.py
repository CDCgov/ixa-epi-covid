from importation.geographies import get_state_proportion_population_data


def test_get_state_proportion_population_data():
    # Test that the function returns a DataFrame with the expected columns and values
    state = "Alabama"
    year = 2020
    result = get_state_proportion_population_data(
        state=state, year=year, cache=False
    )

    for entry in ["Alabama", "alabama", "01", "AL", "al"]:
        assert (
            get_state_proportion_population_data(
                state=entry, year=year, cache=False
            )
            == result
        )
