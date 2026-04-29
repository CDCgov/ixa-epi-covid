from pathlib import Path

from ixa_epi_covid.cloud import cleanup


def test_ensure_az_login_with_identity_passes_resource_id(monkeypatch):
    cleanup._AZ_LOGGED_IN_IDENTITY = cleanup._AZ_NOT_LOGGED_IN
    captured: dict[str, object] = {}

    def fake_ensure_az_login_with_identity(**kwargs):
        captured.update(kwargs)
        return "logged-in"

    monkeypatch.setattr(
        cleanup,
        "_ensure_az_login_with_identity",
        fake_ensure_az_login_with_identity,
    )

    cleanup.ensure_az_login_with_identity(
        managed_identity_resource_id="/subscriptions/test/resourceGroups/rg/providers/Microsoft.ManagedIdentity/userAssignedIdentities/test-id"
    )

    assert (
        captured["managed_identity_resource_id"]
        == "/subscriptions/test/resourceGroups/rg/providers/Microsoft.ManagedIdentity/userAssignedIdentities/test-id"
    )
    assert cleanup._AZ_LOGGED_IN_IDENTITY == "logged-in"


def test_makefile_population_rule_uses_packaged_entrypoint():
    makefile = (Path(__file__).resolve().parents[2] / "Makefile").read_text(
        encoding="utf-8"
    )

    assert "uv run python -m create_synthetic_population.run" in makefile
    assert "scripts/create_synthetic_population.py" not in makefile
