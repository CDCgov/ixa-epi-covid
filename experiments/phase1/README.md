# Running the calibration routine
In order to generate the results analysis report from the `reports/` directory, first calibrate the model by using the phase 1 calibration script. Ensure that the uv environment is synced and then run the calibration to projection pipeline for the phase 1 routine

```
uv sync --all-packages
make projections-phase-1 MAX_WORKERS={MAX_WORKER_COUNT}
```

Then, to render the analysis report, ensure that `tinytex` is installed with

```
quarto install tinytex
```

and then render the document using

```
uv run quarto render experiments/phase1/reports/calibration.qmd
```

The resulting file should be a PDF in the reports directory.
