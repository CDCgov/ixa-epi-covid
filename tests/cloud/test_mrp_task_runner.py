import polars as pl

from ixa_epi_covid.covid_model import CovidModel
from ixa_epi_covid.mrp_task_runner import main as mrp_task_main


def test_covid_model_run_writes_canonical_output(monkeypatch):
    captured: dict[str, object] = {}
    report = pl.DataFrame(
        {
            "t_lower": [0.0, 1.0],
            "t_upper": [1.0, 2.0],
            "count": [0, 1],
        }
    )

    def fake_simulate(model_inputs):
        assert model_inputs == {"seed": 123}
        return {"aggregated_deaths_report": report}

    monkeypatch.setattr(CovidModel, "simulate", staticmethod(fake_simulate))

    monkeypatch.setattr(
        CovidModel, "input", property(lambda self: {"seed": 123})
    )

    model = object.__new__(CovidModel)
    model.write_csv = lambda filename, rows: captured.update(
        {"filename": filename, "rows": rows}
    )

    model.run()

    assert captured["filename"] == "output.csv"
    assert captured["rows"] == {
        "t_lower": [0.0, 1.0],
        "t_upper": [1.0, 2.0],
        "count": [0, 1],
    }


def test_mrp_task_runner_main_delegates(monkeypatch):
    calls: list[str] = []

    monkeypatch.setattr(
        "ixa_epi_covid.mrp_task_runner._covid_model_main",
        lambda: calls.append("called") or 0,
    )

    assert mrp_task_main() == 0
    assert calls == ["called"]
