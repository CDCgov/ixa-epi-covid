from __future__ import annotations

import subprocess
import sys
from pathlib import Path

from calibrationtools.cloud.cleanup import (
    CleanupListing as _CleanupListing,
)
from calibrationtools.cloud.cleanup import CleanupPlan as _CleanupPlan
from calibrationtools.cloud.cleanup import CleanupResult as _CleanupResult
from calibrationtools.cloud.cleanup import build_parser as _build_parser
from calibrationtools.cloud.cleanup import (
    delete_acr_image_tag as _delete_acr_image_tag,
)
from calibrationtools.cloud.cleanup import (
    discover_cleanup_listing as _discover_cleanup_listing,
)
from calibrationtools.cloud.cleanup import (
    discover_cleanup_plan as _discover_cleanup_plan,
)
from calibrationtools.cloud.cleanup import (
    ensure_az_login_with_identity as _ensure_az_login_with_identity,
)
from calibrationtools.cloud.cleanup import execute_cleanup as _execute_cleanup
from calibrationtools.cloud.cleanup import (
    format_cleanup_listing,
    format_cleanup_plan,
)
from calibrationtools.cloud.cleanup import (
    list_acr_repository_tags as _list_acr_repository_tags,
)
from calibrationtools.cloud.cleanup import parse_args as _parse_args
from calibrationtools.cloud.tooling import create_cloud_client, require_tool

from ..mrp_runner import DEFAULT_CLOUD_MRP_CONFIG_PATH
from .utils import CloudRuntimeSettings, load_cloud_runtime_settings

_AZ_NOT_LOGGED_IN = object()
_AZ_LOGGED_IN_IDENTITY: object | str | None = _AZ_NOT_LOGGED_IN


def build_parser():
    return _build_parser(
        default_config_path=DEFAULT_CLOUD_MRP_CONFIG_PATH,
        description=(
            "Clean Azure assets created by one ixa_epi_covid cloud session. "
            "Use --list for read-only discovery across the project naming "
            "scope, or pass --session-slug to inspect or delete one session."
        ),
    )


def parse_args(argv: list[str] | None = None):
    return _parse_args(
        argv,
        default_config_path=DEFAULT_CLOUD_MRP_CONFIG_PATH,
        description=(
            "Clean Azure assets created by one ixa_epi_covid cloud session. "
            "Use --list for read-only discovery across the project naming "
            "scope, or pass --session-slug to inspect or delete one session."
        ),
    )


def discover_cleanup_listing(
    client,
    settings: CloudRuntimeSettings,
    *,
    config_path: Path,
    session_slug: str | None,
    image_tag: str | None,
    include_acr: bool,
    allow_acr_errors: bool = False,
) -> _CleanupListing:
    return _discover_cleanup_listing(
        client,
        settings,
        config_path=config_path,
        session_slug=session_slug,
        image_tag=image_tag,
        include_acr=include_acr,
        allow_acr_errors=allow_acr_errors,
        list_acr_repository_tags_fn=list_acr_repository_tags,
    )


def discover_cleanup_plan(
    client,
    settings: CloudRuntimeSettings,
    *,
    config_path: Path,
    session_slug: str,
    image_tag: str | None,
    include_acr: bool,
) -> _CleanupPlan:
    return _discover_cleanup_plan(
        client,
        settings,
        config_path=config_path,
        session_slug=session_slug,
        image_tag=image_tag,
        include_acr=include_acr,
        list_acr_repository_tags_fn=list_acr_repository_tags,
    )


def execute_cleanup(
    client,
    plan: _CleanupPlan,
    *,
    include_acr: bool,
) -> _CleanupResult:
    return _execute_cleanup(
        client,
        plan,
        include_acr=include_acr,
        delete_acr_image_tag_fn=delete_acr_image_tag,
    )


def list_acr_repository_tags(
    registry_name: str,
    repository_name: str,
    *,
    managed_identity_resource_id: str | None = None,
) -> list[str]:
    return _list_acr_repository_tags(
        registry_name,
        repository_name,
        managed_identity_resource_id=managed_identity_resource_id,
        ensure_az_login_with_identity_func=ensure_az_login_with_identity,
        require_tool_func=require_tool,
        subprocess_run=subprocess.run,
    )


def delete_acr_image_tag(
    registry_name: str,
    repository_name: str,
    image_tag: str,
    *,
    managed_identity_resource_id: str | None = None,
) -> None:
    _delete_acr_image_tag(
        registry_name,
        repository_name,
        image_tag,
        managed_identity_resource_id=managed_identity_resource_id,
        ensure_az_login_with_identity_func=ensure_az_login_with_identity,
        require_tool_func=require_tool,
        subprocess_run=subprocess.run,
    )


def ensure_az_login_with_identity(
    *,
    managed_identity_resource_id: str | None = None,
) -> None:
    global _AZ_LOGGED_IN_IDENTITY
    _AZ_LOGGED_IN_IDENTITY = _ensure_az_login_with_identity(
        managed_identity_resource_id=managed_identity_resource_id,
        current_identity=_AZ_LOGGED_IN_IDENTITY,
        not_logged_in_sentinel=_AZ_NOT_LOGGED_IN,
        require_tool_func=require_tool,
        subprocess_run=subprocess.run,
    )


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    config_path = Path(args.config)
    settings = load_cloud_runtime_settings(config_path)
    client = create_cloud_client(keyvault=settings.keyvault)
    include_acr = not args.skip_acr

    if args.list:
        listing = discover_cleanup_listing(
            client,
            settings,
            config_path=config_path,
            session_slug=args.session_slug,
            image_tag=args.image_tag,
            include_acr=include_acr,
            allow_acr_errors=True,
        )
        print(format_cleanup_listing(listing, include_acr=include_acr))
        return 0

    plan = discover_cleanup_plan(
        client,
        settings,
        config_path=config_path,
        session_slug=args.session_slug,
        image_tag=args.image_tag,
        include_acr=include_acr,
    )
    print(format_cleanup_plan(plan, include_acr=include_acr))

    if not args.yes:
        print(
            "\nDry run only. Re-run with --yes to delete the resources above."
        )
        return 0

    if plan.is_empty:
        print("\nNo matching Azure resources were found.")
        return 0

    result = execute_cleanup(client, plan, include_acr=include_acr)
    print("\nDeleted resources:")
    if result.deleted:
        for item in result.deleted:
            print(f"  - {item}")
    else:
        print("  - none")

    if result.failures:
        print("\nCleanup finished with errors:", file=sys.stderr)
        for failure in result.failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1

    print("\nCleanup finished successfully.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
