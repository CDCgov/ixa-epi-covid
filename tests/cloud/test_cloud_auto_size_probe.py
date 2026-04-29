import polars as pl

from ixa_epi_covid.cloud import auto_size
from ixa_epi_covid.model_execution import CANONICAL_OUTPUT_FILENAME


def test_auto_size_probe_runs_phase1_model_and_writes_canonical_output(
    monkeypatch,
    tmp_path,
):
    calls: dict[str, object] = {}
    report = pl.DataFrame(
        {
            "t_lower": [0.0],
            "t_upper": [1.0],
            "count": [1],
        }
    )

    def fake_execute_phase1_model(model_inputs):
        calls["model_inputs"] = model_inputs
        return {"aggregated_deaths_report": report}

    monkeypatch.setattr(
        auto_size,
        "execute_phase1_model",
        fake_execute_phase1_model,
    )

    base_inputs = {
        "config_inputs": {
            "exe_file": "./target/release/ixa-epi-covid",
            "output_dir": "original-output",
            "outputs_to_read": ["aggregated_deaths_report"],
        },
        "ixa_inputs": {
            "epimodel.GlobalParams": {
                "seed": 1,
                "aggregated_deaths_report": {
                    "filename": "aggregated_deaths_report.csv",
                },
                "imported_cases_timeseries": {
                    "filename": "imported_cases_timeseries.csv",
                },
            }
        },
    }

    auto_size.run_probe_simulation(
        base_inputs,
        "auto-size-probe",
        tmp_path,
    )

    model_inputs = calls["model_inputs"]
    assert model_inputs["config_inputs"]["output_dir"] == str(tmp_path)
    assert model_inputs["ixa_inputs"]["epimodel.GlobalParams"][
        "aggregated_deaths_report"
    ]["filename"] == str(tmp_path / "aggregated_deaths_report.csv")
    assert model_inputs["ixa_inputs"]["epimodel.GlobalParams"][
        "imported_cases_timeseries"
    ]["filename"] == str(tmp_path / "imported_cases_timeseries.csv")
    assert (
        (tmp_path / CANONICAL_OUTPUT_FILENAME)
        .read_text(encoding="utf-8")
        .startswith("t_lower,t_upper,count")
    )
