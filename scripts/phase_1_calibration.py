from ixa_epi_covid.phase1.calibrate import (
    main as cli_main,
)
from ixa_epi_covid.phase1.calibrate import run_phase1_calibration

main = run_phase1_calibration

if __name__ == "__main__":
    raise SystemExit(cli_main())
