# IXA-EPI-COVID

This project presents a general transmission model implemented in ixa, the Center for Forecasting and Outbreak Analytics' agent-based modeling framework. Our goal is to develop a model that can appropriately represent all stages of a respiratory disease outbreak, including case importation, a detailed transmission model accounting for time-varying infectiousness and immunity, and the assessment of non-pharmaceutical interventions. This model is a next generation update of [ixa-epi-isolation](https://github.com/CDCgov/ixa-epi-isolation).

## Project Admin

Community Mitigation and Economic Impacts team of the CDC Center for Forecasting and Outbreak Analytics.
Team lead: Guido Camargo España (CDC/IOD/ORR/CFA)

## Getting Started

This repo uses uv for dependency management, be sure that uv is [installed on your machine](https://docs.astral.sh/uv/getting-started/installation/). To run any python script, you will need to initialize the uv environment first:

```bash
make uv-sync
```

To use this model, you need to have Rust and Cargo installed. You can find instructions for installing Rust [here](https://www.rust-lang.org/tools/install).
To run the main example, use the following command:

```bash
cargo run -- -c input/input.json -o output
```

### Synthetic Population

The model requires a compatible synthetic population CSV in the person-record format supported by `pop_reader`. A small test file is included at `input/people_test.csv`. To generate larger populations, first install R dependencies and set up a [Census Bureau API key](https://api.census.gov/data/key_signup.html) in `.env` as `CENSUS_API_KEY`:

```bash
make setup-r
make synthetic-population             # default: WY, 1000 people
make synthetic-population STATE=NY N=50000  # custom state and size
```

### Make Targets

| Target | Description |
|--------|-------------|
| `make run` | Run the model with default config |
| `make run-1m` | Generate a 1M WY population and run the model with it |
| `make run-10m` | Generate a 10M WY population and run the model with it |
| `make synthetic-population` | Generate a synthetic population (configurable via `STATE` and `N`) |
| `make profile` | Profile the model with [samply](https://github.com/mstange/samply) (configurable, see below) |
| `make setup-r` | Install required R packages |

You can also override the population file directly via CLI with `--synth-population`:

```bash
cargo run --release -- -c input/input.json -o output --synth-population path/to/population.csv
```

### Profiling

`make profile` runs the model under [samply](https://github.com/mstange/samply), which opens the Firefox Profiler with a call tree and flame graph. Press `Ctrl+C` to stop early — samply will still capture the profile. You can also `Ctrl+C` any `make run-*` target to kill the simulation early.

```bash
make profile                                          # 1M population, no ixa spans
make profile PROFILE_SIZE=10m                         # 10M population
make profile PROFILE_FEATURES=profiling               # with ixa span instrumentation
make profile PROFILE_SIZE=10m PROFILE_FEATURES=profiling  # both
```

The `profiling` Cargo feature enables ixa's built-in span timing (`open_span`/`Span::drop`). This adds ~25% overhead, so it's off by default for samply runs. The `run`, `run-1m`, and `run-10m` targets enable it for ixa's own profiling output.

### Calibration

`make calibrate-phase-1-dev` is the easiest way to run the calibration routine of Phase 1, in which we compare the first observed deaths in the model to the first observed death in the State of Indiana. Running this command with setting `SIZE` will generate a toy synthetic population of that size based on Indiana PUMs census data and then run the Phase 1 calibration script. For example, to run routines in popualtions 50K, 100K, and 1 million:

```bash
make calibrate-phase-1-dev SIZE=50_000
make calibrate-phase-1-dev SIZE=100_000
make calibrate-phase-1-dev SIZE=1_000_000
```

By default, the calibration routine uses four parallel worker threads to run simulations. Depending on your computer specs, this can be altered by setting the `MAX_WORKERS` parameter

```bash
make calibrate-phase-1-dev SIZE=50_000 MAX_WORKERS=10
```

To run the post-calibration projection of the accepted particles on a longer time horizon, use the command `make projections-phase-1-dev`, which also accepts the `SIZE` and `MAX_WORKERS` command and will generate the calibration if it deos not exist.

To run the whole routine through generating figures, use

```bash
make plot-phase-1-projection-dev SIZE={n} MAX_WORKERS={m}
```

This command can be called without calling the others explicitly.

Production code follows the same format, but drops the `dev` suffix (for example the calibration command is `make calibrate-phase-1`). To run this, please ensure that you have specified a path to a valid synthetic population in the `.env` file under `SYNTH_POP_FILE`.

## General Disclaimer
This repository was created for use by CDC programs to collaborate on public health related projects in support of the [CDC mission](https://www.cdc.gov/about/organization/mission.htm).  GitHub is not hosted by the CDC, but is a third party website used by CDC and its partners to share information and collaborate on software. CDC use of GitHub does not imply an endorsement of any one particular service, product, or enterprise.

## Public Domain Standard Notice
This repository constitutes a work of the United States Government and is not
subject to domestic copyright protection under 17 USC § 105. This repository is in
the public domain within the United States, and copyright and related rights in
the work worldwide are waived through the [CC0 1.0 Universal public domain dedication](https://creativecommons.org/publicdomain/zero/1.0/).
All contributions to this repository will be released under the CC0 dedication. By
submitting a pull request you are agreeing to comply with this waiver of
copyright interest.

## License Standard Notice
This repository is licensed under ASL v2 or later.

This source code in this repository is free: you can redistribute it and/or modify it under
the terms of the Apache Software License version 2, or (at your option) any
later version.

This source code in this repository is distributed in the hope that it will be useful, but WITHOUT ANY
WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A
PARTICULAR PURPOSE. See the Apache Software License for more details.

You should have received a copy of the Apache Software License along with this
program. If not, see http://www.apache.org/licenses/LICENSE-2.0.html

The source code forked from other open source projects will inherit its license.

## Privacy Standard Notice
This repository contains only non-sensitive, publicly available data and
information. All material and community participation is covered by the
[Disclaimer](https://github.com/CDCgov/template/blob/master/DISCLAIMER.md)
and [Code of Conduct](https://github.com/CDCgov/template/blob/master/code-of-conduct.md).
For more information about CDC's privacy policy, please visit [http://www.cdc.gov/other/privacy.html](https://www.cdc.gov/other/privacy.html).

## Contributing Standard Notice
Anyone is encouraged to contribute to the repository by [forking](https://help.github.com/articles/fork-a-repo)
and submitting a pull request. (If you are new to GitHub, you might start with a
[basic tutorial](https://help.github.com/articles/set-up-git).) By contributing
to this project, you grant a world-wide, royalty-free, perpetual, irrevocable,
non-exclusive, transferable license to all users under the terms of the
[Apache Software License v2](http://www.apache.org/licenses/LICENSE-2.0.html) or
later.

All comments, messages, pull requests, and other submissions received through
CDC including this GitHub page may be subject to applicable federal law, including but not limited to the Federal Records Act, and may be archived. Learn more at [http://www.cdc.gov/other/privacy.html](http://www.cdc.gov/other/privacy.html).

## Records Management Standard Notice
This repository is not a source of government records but is a copy to increase
collaboration and collaborative potential. All government records will be
published through the [CDC web site](http://www.cdc.gov).
