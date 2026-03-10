# Running the calibration routine
In order to generate the results analysis report from the `reports/` directory, first calibrate the model by using the phase 1 calibration script. Ensure that the uv environment is synced and the rust binaries have been assembled
```
uv sync --all-packages
uv run cargo build -r
uv run python scripts/phase_1_calibration.py
```

Then, to render the analysis report, ensure that `tinytex` is installed with

```
quarto install tinyext
```

and then render the document using

```
uv run quarto render experiments/phase1/reports/calibration.qmd
```

The resulting file should be a PDF in the reports directory.
