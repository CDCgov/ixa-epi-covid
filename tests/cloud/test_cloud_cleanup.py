from pathlib import Path

import pytest
from calibrationtools.cloud import cleanup as shared_cleanup


def test_makefile_cloud_cleanup_targets_use_shared_cli():
    makefile = (Path(__file__).resolve().parents[2] / "Makefile").read_text(
        encoding="utf-8"
    )

    assert "cloud-list:" in makefile
    assert "cloud-cleanup-plan:" in makefile
    assert "cloud-cleanup:" in makefile
    assert "calibrationtools.cloud.cleanup" in makefile
    assert "CURRENT_CLOUD_USER := $(shell python3 -c" in makefile
    assert "CLOUD_USER ?= $(CURRENT_CLOUD_USER)" in makefile
    assert "--user $(CLOUD_USER)" in makefile
    assert "--dry-run" in makefile
    assert "--yes" not in makefile
    assert "ixa_epi_covid.cloud.cleanup" not in makefile


def test_shared_cleanup_parser_uses_session_id_and_dry_run():
    args = shared_cleanup.parse_args(
        ["--session-id", "session-1", "--dry-run"],
        default_config_path=Path("ixa_epi_covid.cloud_config.toml"),
    )

    assert args.config == Path("ixa_epi_covid.cloud_config.toml")
    assert args.session_id == "session-1"
    assert args.dry_run is True


def test_shared_cleanup_parser_rejects_legacy_yes_flag():
    with pytest.raises(SystemExit):
        shared_cleanup.parse_args(
            ["--session-id", "session-1", "--yes"],
            default_config_path=Path("ixa_epi_covid.cloud_config.toml"),
        )
