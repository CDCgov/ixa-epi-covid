import inspect


def test_dependency_contract_imports():
    import calibrationtools.cloud
    import calibrationtools.mrp_csv_runner
    from calibrationtools import ABCSampler
    from calibrationtools.cloud.executor import execute_cloud_run
    from calibrationtools.cloud.naming import (
        format_generation_name,
        parse_generation_from_run_id,
        parse_particle_from_run_id,
    )
    from calibrationtools.cloud.runner import (
        CloudMRPRunner,
        create_cloud_mrp_runner,
        create_cloud_mrp_runner_from_config,
        resolve_cloud_build_context,
    )
    from calibrationtools.cloud.task_payload import apply_task_payload_transforms
    from calibrationtools.mrp_csv_runner import CSVOutputMRPRunner, MRPOutputRunner
    from calibrationtools.output_contracts import (
        CSVTableOutputContract,
        OutputContract,
    )

    signature = inspect.signature(ABCSampler)

    assert calibrationtools.cloud is not None
    assert CSVOutputMRPRunner is not None
    assert CloudMRPRunner is not None
    assert execute_cloud_run is not None
    assert create_cloud_mrp_runner is not None
    assert create_cloud_mrp_runner_from_config is not None
    assert resolve_cloud_build_context is not None
    assert MRPOutputRunner is not None
    assert OutputContract is not None
    assert CSVTableOutputContract is not None
    assert apply_task_payload_transforms is not None
    for parameter in (
        "max_concurrent_simulations",
        "print_generation_progress",
        "artifacts_dir",
    ):
        assert parameter in signature.parameters

    assert parse_generation_from_run_id("gen_0_particle_0_attempt_0") == 0
    assert parse_particle_from_run_id("gen_0_particle_0_attempt_0") == 0
    assert format_generation_name(0) == "generation-0"
