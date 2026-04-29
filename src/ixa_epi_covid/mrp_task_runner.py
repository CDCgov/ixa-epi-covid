from __future__ import annotations

from .covid_model import main as _covid_model_main


def main() -> int:
    return _covid_model_main()


if __name__ == "__main__":
    raise SystemExit(main())
