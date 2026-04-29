from importation.etl import get_linelist_data
from importation.geographies import get_total_state_population_data


def test_get_linelist_data_prefers_packaged_resource(monkeypatch, tmp_path):
    monkeypatch.setattr(
        "importation.etl.read_bytes",
        lambda *args, **kwargs: (_ for _ in ()).throw(
            AssertionError("network fetch should not be used")
        ),
    )

    data = get_linelist_data(input_dir=tmp_path, cache=False)

    assert "report_day" in data.columns
    assert "confirmed_cases" in data.columns
    assert data.height > 0


def test_get_total_state_population_data_prefers_packaged_resource(
    monkeypatch,
):
    monkeypatch.setattr(
        "importation.geographies.Census",
        lambda *args, **kwargs: (_ for _ in ()).throw(
            AssertionError("Census API should not be used")
        ),
    )

    data = get_total_state_population_data(year=2020, cache=False)

    assert set(data.columns) == {"state_name", "state", "population"}
    assert data.height > 0
