from __future__ import annotations

from dataclasses import replace
from pathlib import Path
from threading import BoundedSemaphore
from typing import Any

from calibrationtools.cloud.config import DEFAULT_POLL_INTERVAL_SECONDS
from calibrationtools.cloud.runner import CloudMRPRunner
from calibrationtools.cloud.runner import (
    resolve_cloud_build_context as _resolve_cloud_build_context,
)
from calibrationtools.cloud.tooling import upload_files_quietly
from mrp import run as mrp_run

from ..mrp_runner import DEFAULT_CLOUD_MRP_CONFIG_PATH, read_phase1_output_dir
from .utils import cloud_runner_backend, load_cloud_runtime_settings

create_cloud_client = cloud_runner_backend.create_cloud_client
git_short_sha = cloud_runner_backend.git_short_sha
make_session_slug = cloud_runner_backend.make_session_slug
build_local_image = cloud_runner_backend.build_local_image
upload_local_image = cloud_runner_backend.upload_local_image
create_pool_with_blob_mounts = (
    cloud_runner_backend.create_pool_with_blob_mounts
)
wait_for_pool_ready = cloud_runner_backend.wait_for_pool_ready
add_batch_task_with_short_id = (
    cloud_runner_backend.add_batch_task_with_short_id
)
cancel_batch_task = cloud_runner_backend.cancel_batch_task
format_task_failure_message = cloud_runner_backend.format_task_failure_message
format_task_timing_summary = cloud_runner_backend.format_task_timing_summary
make_resource_name = cloud_runner_backend.make_resource_name
parse_generation_from_run_id = (
    cloud_runner_backend.parse_generation_from_run_id
)
suppress_cloudops_info_output = (
    cloud_runner_backend.suppress_cloudops_info_output
)

_DEFAULT_REPO_ROOT = Path(__file__).resolve().parents[3]
_DEFAULT_DOCKERFILE_RELATIVE_PATH = Path("Dockerfile.cloud")
_CLOUD_TASK_EXE_FILE = "/usr/local/bin/ixa-epi-covid"
_CLOUD_TASK_OUTPUT_ROOT = Path("/tmp/ixa-epi-covid")
_MAX_PARALLEL_OUTPUT_DOWNLOADS = 8


class _SharedCloudClientProxy:
    """Per-run proxy that reuses the shared Azure service clients.

    The base cloud runner creates a fresh ``CloudClient`` for each submission,
    wait loop, and download. At higher concurrency that fans out into many
    simultaneous auth/client bootstrap calls. Reuse the already-initialized
    service clients from ``self.client`` instead, while keeping per-run
    ``logs_folder`` and ``save_logs_to_blob`` fields isolated on the proxy.
    """

    def __init__(self, shared_client: Any) -> None:
        self._shared_client = shared_client
        self.batch_service_client = shared_client.batch_service_client
        self.batch_mgmt_client = shared_client.batch_mgmt_client
        self.blob_service_client = shared_client.blob_service_client
        self.compute_mgmt_client = getattr(
            shared_client,
            "compute_mgmt_client",
            None,
        )
        self.cred = shared_client.cred
        self.full_container_name = getattr(
            shared_client,
            "full_container_name",
            None,
        )
        self.save_logs_to_blob = getattr(
            shared_client,
            "save_logs_to_blob",
            None,
        )
        self.logs_folder = getattr(
            shared_client,
            "logs_folder",
            "stdout_stderr",
        )

    def download_file(self, *args: Any, **kwargs: Any) -> Any:
        return self._shared_client.download_file(*args, **kwargs)

    def __getattr__(self, name: str) -> Any:
        return getattr(self._shared_client, name)


def _current_cloud_runner_backend():
    return replace(
        cloud_runner_backend,
        create_cloud_client=create_cloud_client,
        git_short_sha=git_short_sha,
        make_session_slug=make_session_slug,
        build_local_image=build_local_image,
        upload_local_image=upload_local_image,
        create_pool_with_blob_mounts=create_pool_with_blob_mounts,
        wait_for_pool_ready=wait_for_pool_ready,
        add_batch_task_with_short_id=add_batch_task_with_short_id,
        cancel_batch_task=cancel_batch_task,
        format_task_failure_message=format_task_failure_message,
        format_task_timing_summary=format_task_timing_summary,
        make_resource_name=make_resource_name,
        parse_generation_from_run_id=parse_generation_from_run_id,
        suppress_cloudops_info_output=suppress_cloudops_info_output,
    )


def resolve_cloud_build_context(
    repo_root: str | Path | None = None,
    dockerfile: str | Path | None = None,
) -> tuple[Path, Path]:
    return _resolve_cloud_build_context(
        default_repo_root=_DEFAULT_REPO_ROOT,
        default_dockerfile_relative_path=_DEFAULT_DOCKERFILE_RELATIVE_PATH,
        repo_root=repo_root,
        dockerfile=dockerfile,
        missing_dockerfile_message=(
            "Cloud mode requires Dockerfile.cloud. "
            "Looked at {dockerfile}; pass --repo-root and --dockerfile "
            "when running from an installed wheel."
        ),
    )


class _Phase1CloudMRPRunner(CloudMRPRunner):
    """Cloud MRP runner with shared synthetic-population staging."""

    def __init__(
        self,
        config_path: str | Path = DEFAULT_CLOUD_MRP_CONFIG_PATH,
        *,
        generation_count: int,
        max_concurrent_simulations: int,
        synth_population_path: str | Path,
        repo_root: str | Path | None = None,
        dockerfile: str | Path | None = None,
        print_task_durations: bool = False,
        task_slots_per_node_override: int | None = None,
        auto_size_summary: Any | None = None,
    ) -> None:
        self._shared_population_local_path = Path(synth_population_path)
        if not self._shared_population_local_path.exists():
            raise FileNotFoundError(
                "Synthetic population file required for cloud mode was not "
                f"found: {self._shared_population_local_path}"
            )
        self._max_parallel_output_downloads = min(
            max_concurrent_simulations,
            _MAX_PARALLEL_OUTPUT_DOWNLOADS,
        )
        self._output_download_semaphore = BoundedSemaphore(
            self._max_parallel_output_downloads
        )

        resolved_repo_root, resolved_dockerfile = resolve_cloud_build_context(
            repo_root=repo_root,
            dockerfile=dockerfile,
        )
        self._shared_population_blob_dir = "shared"
        self._shared_population_blob_name = (
            self._shared_population_local_path.name
        )

        settings_loader = load_cloud_runtime_settings
        if task_slots_per_node_override is not None:

            def settings_loader(config_path: str | Path):
                settings = load_cloud_runtime_settings(config_path)
                return replace(
                    settings,
                    task_slots_per_node=task_slots_per_node_override,
                )

        runner_kwargs: dict[str, Any] = {}
        if auto_size_summary is not None:
            runner_kwargs["auto_size_summary"] = auto_size_summary

        super().__init__(
            config_path,
            generation_count=generation_count,
            max_concurrent_simulations=max_concurrent_simulations,
            repo_root=resolved_repo_root,
            dockerfile=resolved_dockerfile,
            settings_loader=settings_loader,
            read_output_dir=read_phase1_output_dir,
            output_filename="output.csv",
            print_task_durations=print_task_durations,
            backend=_current_cloud_runner_backend(),
            poll_interval_seconds=DEFAULT_POLL_INTERVAL_SECONDS,
            mrp_run_func=mrp_run,
            **runner_kwargs,
        )
        self._create_cloud_client = self._create_shared_cloud_client
        self._shared_population_remote_path = (
            f"{self.session.input_mount_path.rstrip('/')}/"
            f"{self._shared_population_blob_dir}/"
            f"{self._shared_population_blob_name}"
        )
        self._stage_shared_population()

    def _create_shared_cloud_client(
        self,
        *,
        keyvault: str,
    ) -> _SharedCloudClientProxy:
        del keyvault
        return _SharedCloudClientProxy(self.client)

    def _stage_shared_population(self) -> None:
        with self._suppress_cloudops_info_output():
            upload_files_quietly(
                self.client,
                files=self._shared_population_blob_name,
                container_name=self.session.input_container,
                local_root_dir=str(
                    self._shared_population_local_path.parent.resolve()
                ),
                location_in_blob=self._shared_population_blob_dir,
            )

    def _resolve_input_payload(
        self,
        params: dict[str, Any],
        *,
        input_path: str | Path | None,
        run_id: str,
    ) -> dict[str, Any]:
        payload = super()._resolve_input_payload(
            params,
            input_path=input_path,
            run_id=run_id,
        )
        task_output_dir = _CLOUD_TASK_OUTPUT_ROOT / run_id
        try:
            payload["config_inputs"]["exe_file"] = _CLOUD_TASK_EXE_FILE
            payload["config_inputs"]["output_dir"] = str(task_output_dir)
            payload["ixa_inputs"]["epimodel.GlobalParams"][
                "synth_population_file"
            ] = self._shared_population_remote_path
            payload["ixa_inputs"]["epimodel.GlobalParams"][
                "imported_cases_timeseries"
            ]["filename"] = str(
                task_output_dir / "imported_cases_timeseries.csv"
            )
        except KeyError as exc:
            raise ValueError(
                "Phase-1 cloud payload is missing one of "
                "config_inputs.exe_file, config_inputs.output_dir, "
                "ixa_inputs.epimodel.GlobalParams.synth_population_file, or "
                "ixa_inputs.epimodel.GlobalParams."
                "imported_cases_timeseries.filename"
            ) from exc
        return payload

    def _download_output_blocking(
        self,
        run_id: str,
        output_dir: Path,
    ) -> float:
        # Bound host-side blob downloads so high cloud concurrency does not
        # stampede local file handles and sockets when many tasks complete at
        # once.
        with self._output_download_semaphore:
            return super()._download_output_blocking(run_id, output_dir)


def IxaEpiCovidCloudRunner(
    config_path: str | Path = DEFAULT_CLOUD_MRP_CONFIG_PATH,
    *,
    generation_count: int,
    max_concurrent_simulations: int,
    synth_population_path: str | Path,
    repo_root: str | Path | None = None,
    dockerfile: str | Path | None = None,
    print_task_durations: bool = False,
    task_slots_per_node_override: int | None = None,
    auto_size_summary: Any | None = None,
):
    return _Phase1CloudMRPRunner(
        config_path,
        generation_count=generation_count,
        max_concurrent_simulations=max_concurrent_simulations,
        synth_population_path=synth_population_path,
        repo_root=repo_root,
        dockerfile=dockerfile,
        print_task_durations=print_task_durations,
        task_slots_per_node_override=task_slots_per_node_override,
        auto_size_summary=auto_size_summary,
    )
