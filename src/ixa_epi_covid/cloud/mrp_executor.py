from __future__ import annotations

from dataclasses import replace

from calibrationtools.cloud.executor import (
    execute_cloud_run as _execute_cloud_run,
)
from calibrationtools.cloud.executor import read_run_json as _read_run_json

from .utils import cloud_executor_backend

create_cloud_client = cloud_executor_backend.create_cloud_client
add_batch_task_with_short_id = (
    cloud_executor_backend.add_batch_task_with_short_id
)
wait_for_task_completion = cloud_executor_backend.wait_for_task_completion
cancel_batch_task = cloud_executor_backend.cancel_batch_task
format_task_failure_message = (
    cloud_executor_backend.format_task_failure_message
)
format_task_timing_summary = cloud_executor_backend.format_task_timing_summary
suppress_cloudops_info_output = (
    cloud_executor_backend.suppress_cloudops_info_output
)


def _current_cloud_executor_backend():
    return replace(
        cloud_executor_backend,
        create_cloud_client=create_cloud_client,
        add_batch_task_with_short_id=add_batch_task_with_short_id,
        wait_for_task_completion=wait_for_task_completion,
        cancel_batch_task=cancel_batch_task,
        format_task_failure_message=format_task_failure_message,
        format_task_timing_summary=format_task_timing_summary,
        suppress_cloudops_info_output=suppress_cloudops_info_output,
    )


def execute_cloud_run(run_json):
    return _execute_cloud_run(
        run_json,
        output_filename="output.csv",
        backend=_current_cloud_executor_backend(),
    )


def main() -> int:
    execute_cloud_run(_read_run_json())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
