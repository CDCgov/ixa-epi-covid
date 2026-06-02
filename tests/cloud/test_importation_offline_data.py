from importation import etl, geographies


def test_packaged_linelist_data_is_preferred(monkeypatch, tmp_path):
    def fail_network(*args, **kwargs):
        raise AssertionError("network download should not be used")

    monkeypatch.setattr(etl, "read_bytes", fail_network)

    data = etl.get_linelist_data(input_dir=tmp_path)

    assert data.height > 0
    assert {"report_day", "confirmed_cases", "confirmed_deaths"} <= set(
        data.columns
    )


def test_packaged_state_population_data_is_preferred(monkeypatch, tmp_path):
    class FailingCensus:
        def __init__(self, *args, **kwargs):
            raise AssertionError("Census API should not be used")

    monkeypatch.chdir(tmp_path)
    monkeypatch.setattr(geographies, "Census", FailingCensus)

    data = geographies.get_total_state_population_data(year=2020)

    assert data.filter(data["state_name"] == "Indiana").height == 1
