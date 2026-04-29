import time
import tomllib
from concurrent.futures import ThreadPoolExecutor
from contextlib import nullcontext
from dataclasses import replace
from pathlib import Path
from threading import Event, Lock
from types import SimpleNamespace

import pytest

from ixa_epi_covid.cloud.runner import (
    _MAX_PARALLEL_OUTPUT_DOWNLOADS,
    _Phase1CloudMRPRunner,
)
from ixa_epi_covid.cloud.utils import DEFAULT_CLOUD_RUNTIME_SETTINGS


def test_cloud_runner_rejects_missing_population_before_provisioning(
    monkeypatch,
    tmp_path,
):
    monkeypatch.setattr(
        "calibrationtools.cloud.runner.CloudMRPRunner.__init__",
        lambda *args, **kwargs: (_ for _ in ()).throw(
            AssertionError("cloud resources should not be initialized")
        ),
    )
    dockerfile = tmp_path / "Dockerfile.cloud"
    dockerfile.write_text("FROM scratch\n", encoding="utf-8")

    with pytest.raises(FileNotFoundError):
        _Phase1CloudMRPRunner(
            generation_count=1,
            max_concurrent_simulations=1,
            synth_population_path=tmp_path / "missing.csv",
            repo_root=tmp_path,
            dockerfile=dockerfile,
        )


def test_cloud_runner_stages_shared_population_and_rewrites_payload(
    monkeypatch,
    tmp_path,
):
    synth_population = tmp_path / "synth.csv"
    synth_population.write_text("person_id\n1\n", encoding="utf-8")
    dockerfile = tmp_path / "Dockerfile.cloud"
    dockerfile.write_text("FROM scratch\n", encoding="utf-8")
    uploads: list[dict[str, object]] = []

    monkeypatch.setattr(
        "ixa_epi_covid.cloud.runner.resolve_cloud_build_context",
        lambda repo_root=None, dockerfile=None: (tmp_path, Path(dockerfile)),
    )

    def fake_super_init(
        self,
        config_path,
        *,
        generation_count,
        max_concurrent_simulations,
        repo_root,
        dockerfile,
        settings_loader,
        read_output_dir,
        output_filename,
        print_task_durations,
        backend,
        poll_interval_seconds,
        mrp_run_func,
    ):
        self.session = SimpleNamespace(
            input_mount_path="/cloud-input",
            input_container="input-container",
        )
        self.client = object()
        self._suppress_cloudops_info_output = nullcontext

    monkeypatch.setattr(
        "calibrationtools.cloud.runner.CloudMRPRunner.__init__",
        fake_super_init,
    )
    monkeypatch.setattr(
        "ixa_epi_covid.cloud.runner.upload_files_quietly",
        lambda client, **kwargs: uploads.append(kwargs),
    )

    runner = _Phase1CloudMRPRunner(
        generation_count=1,
        max_concurrent_simulations=1,
        synth_population_path=synth_population,
        repo_root=tmp_path,
        dockerfile=dockerfile,
    )

    assert len(uploads) == 1
    assert uploads[0]["container_name"] == "input-container"
    assert uploads[0]["location_in_blob"] == "shared"
    assert uploads[0]["files"] == "synth.csv"

    payload = runner._resolve_input_payload(
        {
            "config_inputs": {
                "exe_file": "./target/release/ixa-epi-covid",
                "output_dir": "/tmp/original-output",
            },
            "ixa_inputs": {
                "epimodel.GlobalParams": {
                    "imported_cases_timeseries": {
                        "filename": "/tmp/original-output/imported_cases_timeseries.csv"
                    },
                    "synth_population_file": "local.csv",
                }
            },
        },
        input_path=None,
        run_id="gen_0_particle_0_attempt_0",
    )

    assert (
        payload["config_inputs"]["exe_file"] == "/usr/local/bin/ixa-epi-covid"
    )
    assert (
        payload["config_inputs"]["output_dir"]
        == "/tmp/ixa-epi-covid/gen_0_particle_0_attempt_0"
    )
    assert (
        payload["ixa_inputs"]["epimodel.GlobalParams"]["synth_population_file"]
        == "/cloud-input/shared/synth.csv"
    )
    assert (
        payload["ixa_inputs"]["epimodel.GlobalParams"][
            "imported_cases_timeseries"
        ]["filename"]
        == "/tmp/ixa-epi-covid/gen_0_particle_0_attempt_0/imported_cases_timeseries.csv"
    )


def test_task_mrp_config_uses_packaged_python():
    config_path = (
        Path(__file__).resolve().parents[2] / "ixa_epi_covid.mrp.task.toml"
    )
    with config_path.open("rb") as fp:
        config = tomllib.load(fp)

    assert config["runtime"]["command"] == "/app/.venv/bin/python"


def test_cloud_runner_applies_task_slots_override_to_settings_loader(
    monkeypatch,
    tmp_path,
):
    synth_population = tmp_path / "synth.csv"
    synth_population.write_text("person_id\n1\n", encoding="utf-8")
    dockerfile = tmp_path / "Dockerfile.cloud"
    dockerfile.write_text("FROM scratch\n", encoding="utf-8")
    loaded_settings = []

    monkeypatch.setattr(
        "ixa_epi_covid.cloud.runner.resolve_cloud_build_context",
        lambda repo_root=None, dockerfile=None: (tmp_path, Path(dockerfile)),
    )
    monkeypatch.setattr(
        "ixa_epi_covid.cloud.runner.load_cloud_runtime_settings",
        lambda config_path: replace(
            DEFAULT_CLOUD_RUNTIME_SETTINGS,
            task_slots_per_node=50,
        ),
    )

    def fake_super_init(
        self,
        config_path,
        *,
        generation_count,
        max_concurrent_simulations,
        repo_root,
        dockerfile,
        settings_loader,
        read_output_dir,
        output_filename,
        print_task_durations,
        backend,
        poll_interval_seconds,
        mrp_run_func,
        auto_size_summary=None,
    ):
        loaded_settings.append(settings_loader(config_path))
        self.session = SimpleNamespace(
            input_mount_path="/cloud-input",
            input_container="input-container",
        )
        self.client = object()
        self._suppress_cloudops_info_output = nullcontext

    monkeypatch.setattr(
        "calibrationtools.cloud.runner.CloudMRPRunner.__init__",
        fake_super_init,
    )
    monkeypatch.setattr(
        "ixa_epi_covid.cloud.runner.upload_files_quietly",
        lambda client, **kwargs: None,
    )

    _Phase1CloudMRPRunner(
        generation_count=1,
        max_concurrent_simulations=40,
        synth_population_path=synth_population,
        repo_root=tmp_path,
        dockerfile=dockerfile,
        task_slots_per_node_override=9,
    )

    assert loaded_settings[0].task_slots_per_node == 9


def test_cloud_runner_limits_parallel_output_downloads(
    monkeypatch,
    tmp_path,
):
    synth_population = tmp_path / "synth.csv"
    synth_population.write_text("person_id\n1\n", encoding="utf-8")
    dockerfile = tmp_path / "Dockerfile.cloud"
    dockerfile.write_text("FROM scratch\n", encoding="utf-8")
    start_event = Event()
    active_downloads = 0
    max_active_downloads = 0
    lock = Lock()

    monkeypatch.setattr(
        "ixa_epi_covid.cloud.runner.resolve_cloud_build_context",
        lambda repo_root=None, dockerfile=None: (tmp_path, Path(dockerfile)),
    )

    def fake_super_init(
        self,
        config_path,
        *,
        generation_count,
        max_concurrent_simulations,
        repo_root,
        dockerfile,
        settings_loader,
        read_output_dir,
        output_filename,
        print_task_durations,
        backend,
        poll_interval_seconds,
        mrp_run_func,
    ):
        self.session = SimpleNamespace(
            input_mount_path="/cloud-input",
            input_container="input-container",
        )
        self.client = object()
        self._suppress_cloudops_info_output = nullcontext

    monkeypatch.setattr(
        "calibrationtools.cloud.runner.CloudMRPRunner.__init__",
        fake_super_init,
    )
    monkeypatch.setattr(
        "ixa_epi_covid.cloud.runner.upload_files_quietly",
        lambda client, **kwargs: None,
    )

    def fake_super_download(self, run_id, output_dir):
        nonlocal active_downloads, max_active_downloads
        with lock:
            active_downloads += 1
            max_active_downloads = max(
                max_active_downloads,
                active_downloads,
            )
        time.sleep(0.05)
        with lock:
            active_downloads -= 1
        return 0.01

    monkeypatch.setattr(
        "calibrationtools.cloud.runner.CloudMRPRunner._download_output_blocking",
        fake_super_download,
    )

    runner = _Phase1CloudMRPRunner(
        generation_count=1,
        max_concurrent_simulations=40,
        synth_population_path=synth_population,
        repo_root=tmp_path,
        dockerfile=dockerfile,
    )

    def run_download(index: int) -> float:
        start_event.wait(timeout=1)
        return runner._download_output_blocking(
            f"gen_0_particle_{index}_attempt_0",
            tmp_path / f"run-{index}",
        )

    with ThreadPoolExecutor(max_workers=16) as pool:
        futures = [pool.submit(run_download, index) for index in range(16)]
        start_event.set()
        for future in futures:
            assert future.result() == 0.01

    assert runner._max_parallel_output_downloads == min(
        40,
        _MAX_PARALLEL_OUTPUT_DOWNLOADS,
    )
    assert max_active_downloads == runner._max_parallel_output_downloads


def test_cloud_runner_reuses_shared_client_services_per_proxy(
    monkeypatch,
    tmp_path,
):
    synth_population = tmp_path / "synth.csv"
    synth_population.write_text("person_id\n1\n", encoding="utf-8")
    dockerfile = tmp_path / "Dockerfile.cloud"
    dockerfile.write_text("FROM scratch\n", encoding="utf-8")

    class FakeSharedClient:
        def __init__(self) -> None:
            self.batch_service_client = object()
            self.batch_mgmt_client = object()
            self.blob_service_client = object()
            self.compute_mgmt_client = object()
            self.cred = object()
            self.full_container_name = "example/image:tag"
            self.save_logs_to_blob = None
            self.logs_folder = "stdout_stderr"
            self.download_calls: list[dict[str, object]] = []

        def download_file(self, *args, **kwargs):
            self.download_calls.append(
                {
                    "args": args,
                    "kwargs": kwargs,
                }
            )

    shared_client = FakeSharedClient()

    monkeypatch.setattr(
        "ixa_epi_covid.cloud.runner.resolve_cloud_build_context",
        lambda repo_root=None, dockerfile=None: (tmp_path, Path(dockerfile)),
    )

    def fake_super_init(
        self,
        config_path,
        *,
        generation_count,
        max_concurrent_simulations,
        repo_root,
        dockerfile,
        settings_loader,
        read_output_dir,
        output_filename,
        print_task_durations,
        backend,
        poll_interval_seconds,
        mrp_run_func,
    ):
        self.session = SimpleNamespace(
            input_mount_path="/cloud-input",
            input_container="input-container",
        )
        self.client = shared_client
        self._suppress_cloudops_info_output = nullcontext

    monkeypatch.setattr(
        "calibrationtools.cloud.runner.CloudMRPRunner.__init__",
        fake_super_init,
    )
    monkeypatch.setattr(
        "ixa_epi_covid.cloud.runner.upload_files_quietly",
        lambda client, **kwargs: None,
    )

    runner = _Phase1CloudMRPRunner(
        generation_count=1,
        max_concurrent_simulations=40,
        synth_population_path=synth_population,
        repo_root=tmp_path,
        dockerfile=dockerfile,
    )

    proxy_a = runner._create_cloud_client(keyvault="ignored")
    proxy_b = runner._create_cloud_client(keyvault="ignored")

    assert proxy_a is not proxy_b
    assert proxy_a.batch_service_client is shared_client.batch_service_client
    assert proxy_b.blob_service_client is shared_client.blob_service_client

    proxy_a.logs_folder = "run-a"
    proxy_b.logs_folder = "run-b"
    assert shared_client.logs_folder == "stdout_stderr"
    assert proxy_a.logs_folder == "run-a"
    assert proxy_b.logs_folder == "run-b"

    proxy_a.download_file(
        src_path="example.txt",
        dest_path="out.txt",
        container_name="container",
    )
    assert shared_client.download_calls == [
        {
            "args": (),
            "kwargs": {
                "src_path": "example.txt",
                "dest_path": "out.txt",
                "container_name": "container",
            },
        }
    ]
