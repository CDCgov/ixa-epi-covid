import pytest

from ixa_epi_covid.cloud.utils import (
    DEFAULT_CLOUD_RUNTIME_SETTINGS,
    load_cloud_runtime_settings,
)


def test_load_cloud_runtime_settings_layers_defaults(tmp_path):
    config_path = tmp_path / "cloud.toml"
    config_path.write_text(
        """
        [runtime.cloud]
        repository = "custom-repository"
        jobs_per_session = 3
        """,
        encoding="utf-8",
    )

    settings = load_cloud_runtime_settings(config_path)

    assert settings.keyvault == DEFAULT_CLOUD_RUNTIME_SETTINGS.keyvault
    assert settings.repository == "custom-repository"
    assert settings.jobs_per_session == 3


def test_load_cloud_runtime_settings_accepts_legacy_jobs_per_generation(
    tmp_path,
):
    config_path = tmp_path / "cloud.toml"
    config_path.write_text(
        """
        [runtime.cloud]
        jobs_per_generation = 2
        """,
        encoding="utf-8",
    )

    with pytest.deprecated_call():
        settings = load_cloud_runtime_settings(config_path)

    assert settings.jobs_per_session == 2
