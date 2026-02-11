# Importation
Package for generating input files that determine infection importation rates.

- ETL functions to process raw case data into a format suitable for modeling.
- Functions to generate synthetic datasets of undetected infections based on observed cases and deaths.

## Background
This package depends on collecting data from *Perkins et al. Estimating unobserved SARS-CoV-2 infection in the United States. PNAS (2020).*
The raw linelist data and importation data are available through the associated code repository of the paper, which can be used
to re-create the national US importation model in the paper. From there, state-level importation models can be developed.

## Getting started
Be sure to install `quarto` and `tinytex` in order  to render example docs.
Follow errors when running the following commands for assistance in installing the software.

To make the example documentation, run

```
uv run quarto render packages/importation/docs/example_indiana.qmd
```

from the root directory or
```
uv run quarto render example_indiana.qmd
```

from the importation `docs` directory.
