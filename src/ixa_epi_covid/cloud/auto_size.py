from __future__ import annotations

import argparse
import copy
import json
import resource
import sys
from pathlib import Path
from typing import Any

from ixa_epi_covid.model_execution import (
    CANONICAL_OUTPUT_FILENAME,
    execute_phase1_model,
    write_canonical_output_csv,
)


def run_probe_simulation(
    base_inputs: dict[str, Any],
    run_id: str,
    output_dir: Path,
) -> None:
    """Run one phase-1 simulation for local cloud auto-sizing."""
    model_inputs = copy.deepcopy(base_inputs)
    model_inputs["run_id"] = run_id
    config_inputs = model_inputs["config_inputs"]
    config_inputs["output_dir"] = str(output_dir)

    global_params = model_inputs["ixa_inputs"]["epimodel.GlobalParams"]
    for output_name in config_inputs["outputs_to_read"]:
        global_params[output_name]["filename"] = str(
            output_dir / Path(global_params[output_name]["filename"]).name
        )
    global_params["imported_cases_timeseries"]["filename"] = str(
        output_dir
        / Path(global_params["imported_cases_timeseries"]["filename"]).name
    )

    outputs = execute_phase1_model(model_inputs)
    write_canonical_output_csv(
        output_dir / CANONICAL_OUTPUT_FILENAME,
        outputs,
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--child", action="store_true")
    args = parser.parse_args()
    if not args.child:
        raise SystemExit("cloud auto-size probe helpers are internal")
    _run_memory_probe_child()


def _run_memory_probe_child() -> None:
    request = json.loads(sys.stdin.read())
    if not isinstance(request, dict):
        raise ValueError("probe request must be a JSON object")

    base_inputs = request.get("base_inputs")
    if not isinstance(base_inputs, dict):
        raise ValueError("probe request must include object base_inputs")

    run_id = request.get("run_id")
    if not isinstance(run_id, str) or not run_id:
        raise ValueError("probe request must include non-empty run_id")

    output_dir_value = request.get("output_dir")
    if not isinstance(output_dir_value, str) or not output_dir_value:
        raise ValueError("probe request must include non-empty output_dir")

    output_dir = Path(output_dir_value)
    output_dir.mkdir(parents=True, exist_ok=True)
    run_probe_simulation(base_inputs, run_id, output_dir)
    print(json.dumps({"peak_rss_bytes": _peak_rss_bytes()}), flush=True)


def _peak_rss_bytes() -> int:
    self_rss = int(resource.getrusage(resource.RUSAGE_SELF).ru_maxrss)
    child_rss = int(resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss)
    if sys.platform == "darwin":
        return self_rss + child_rss
    return (self_rss + child_rss) * 1024


if __name__ == "__main__":
    main()
