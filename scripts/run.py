"""Run one deliberately small Azure Batch calibration using ``example-model``."""

from __future__ import annotations

import argparse
from collections.abc import Sequence
from typing import Any

import os
from pathlib import Path

import numpy as np
from numpy.random import default_rng
from mrp import Environment, MRPModel

from calibrationtools.azure_batch_executor import AzureBatchExecutor
from calibrationtools import (
    CalibrationScenario,
    CalibrationStudy,
)
from calibrationtools.perturbation_kernel import (
    IndependentKernels,
    NormalKernel,
    SeedKernel,
)
from calibrationtools.sampler import ABCSampler
from calibrationtools.variance_adapter import AdaptIdentityVariance
from dotenv import load_dotenv
import os

# import os
# from pathlib import Path
# from dotenv import find_dotenv, dotenv_values, load_dotenv

# print("cwd:", Path.cwd())
# print("dotenv file:", find_dotenv())

# dotenv_path = find_dotenv()
# print(
#     "tenant in dotenv:",
#     repr(dotenv_values(dotenv_path).get("AZURE_TENANT_ID"))
# )
# print(
#     "tenant in process before load:",
#     repr(os.getenv("AZURE_TENANT_ID"))
# )

# load_dotenv(dotenv_path, override=True)

# print(
#     "tenant in process after load:",
#     repr(os.getenv("AZURE_TENANT_ID"))
# )
# print("authority:", repr(os.getenv("AZURE_AUTHORITY_HOST")))

class Binom_BP_Model(MRPModel):
    def run(self):
        print("Running Binom_BP_Model with input:", self.input)
        results = self.simulate(self.input)
        self.write_csv(
            "output.csv",
            {
                "generation": list(range(len(results))),
                "population": results,
            },
        )

    @staticmethod
    def simulate(model_inputs: dict[str, Any]) -> list[int]:
        """
        Galton-Watson branching process model with binomial reproduction.

        Simulates population growth where each individual reproduces independently
        according to a binomial distribution. Starting with 1 individual, the model
        runs for multiple generations until reaching max_gen or max_infect threshold.

        Args:
            model_inputs: Dictionary containing model parameters:
                seed (int, optional): Random seed for reproducibility. If not provided, uses random seed.
                max_gen (int): Maximum number of generations to simulate
                n (int): Number of trials in binomial distribution (max offspring per individual)
                p (float): Success probability in binomial distribution (0 to 1)
                max_infect (int): Population threshold to stop simulation early

        Returns:
            list[int]: Population size at each generation, starting from generation 0

        Examples:
            >>> inputs = {"seed": 123, "max_gen": 15, "n": 3, "p": 0.5, "max_infect": 500}
            >>> model = Binom_BP_Model(inputs)
            >>> model.run()
            >>> len(model.simulate(inputs))  # Number of generations simulated
        """
        seed = model_inputs.get("seed", None)
        rng = default_rng(seed)
        out: list[int] = []
        for i in range(model_inputs["max_gen"]):
            if i == 0:
                out.append(1)
            else:
                next = int(
                    sum(
                        rng.binomial(
                            model_inputs["n"], model_inputs["p"], out[i - 1]
                        )
                    )
                )
                out.append(next)
                if next > model_inputs["max_infect"]:
                    break
        return out


DEFAULT_PARAMETERS: dict[str, int | float] = {
    "seed": 123,
    "max_gen": 8,
    "n": 3,
    "p": 0.5,
    "max_infect": 100,
}
PRIORS: dict[str, Any] = {
    "priors": {
        "p": {
            "distribution": "uniform",
            "parameters": {"min": 0.0, "max": 1.0},
        }
    }
}
SMOKE_TOLERANCE = 10_000.0


def outputs_to_distance(model_output: list[int], target_data: float) -> float:
    """Score one branching-process output against the small smoke target."""

    return abs(float(np.sum(model_output)) - target_data)


def build_smoke_sampler(
    particle_count: int = 4, target_data: float = 5.0
) -> ABCSampler:
    """Build a one-generation sampler that finishes in one attempt per slot.

    The broad, finite tolerance makes this an infrastructure check rather than
    a statistically meaningful calibration. The top-level functions and model
    class are importable in the worker image, so the normal Azure task-pickle
    preflight also exercises the intended deployment boundary.
    """

    if particle_count <= 0:
        raise ValueError("particle_count must be positive")

    return ABCSampler(
        generation_particle_count=particle_count,
        tolerance_values=[SMOKE_TOLERANCE],
        priors=PRIORS,
        perturbation_kernel=IndependentKernels(
            [NormalKernel("p", 0.25), SeedKernel("seed")]
        ),
        variance_adapter=AdaptIdentityVariance(),
        default_parameters=DEFAULT_PARAMETERS,
        outputs_to_distance=outputs_to_distance,
        target_data=target_data,
        model_runner=Binom_BP_Model(
            env=Environment({"input": dict(DEFAULT_PARAMETERS)})
        ),
        max_attempts_per_proposal=1,
        entropy=123,
        verbose=False,
    )


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse Azure resource and cost-control settings for the smoke run."""

    parser = argparse.ArgumentParser(
        description=(
            "Run the bundled branching-process model through one small Azure "
            "Batch generation."
        )
    )
    parser.add_argument("--base-name", default="example-model-smoke")
    parser.add_argument(
        "--registry-server",
        help=(
            "ACR server, e.g. myregistry.azurecr.io; defaults to "
            "AZURE_CONTAINER_REGISTRY_SERVER."
        ),
    )
    parser.add_argument(
        "--image-name", default="calibrationtools-example-smoke"
    )
    parser.add_argument("--image-tag", default="smoke")
    parser.add_argument(
        "--image-dockerfile",
        default="Dockerfile",
        help="Dockerfile path interpreted from the repository root.",
    )
    parser.add_argument("--particle-count", type=int, default=4)
    parser.add_argument("--chunk-size", type=int, default=1)
    parser.add_argument("--max-autoscale-nodes", type=int, default=1)
    parser.add_argument(
        "--study",
        action="store_true",
        help="Run two named smoke scenarios concurrently through Azure Batch.",
    )
    parser.add_argument("--max-concurrent-scenarios", type=int, default=2)
    parser.add_argument(
        "--detail-log-dir",
        default="azure-smoke-study-logs",
        help="Directory for per-scenario JSONL progress logs in study mode.",
    )
    parser.add_argument("--poll-interval", type=float, default=5.0)
    parser.add_argument("--max-wait", type=float, default=1800.0)
    image_actions = parser.add_mutually_exclusive_group()
    image_actions.add_argument("--build-image", action="store_true")
    image_actions.add_argument("--upload-image", action="store_true")
    parser.add_argument(
        "--keep-job",
        action="store_false",
        dest="delete_job_after",
        help="Retain the Batch job after completion for inspection.",
    )
    parser.add_argument(
        "--keep-pool",
        action="store_false",
        dest="delete_pool_after",
        help="Retain the Batch pool after completion for inspection.",
    )
    parser.set_defaults(delete_job_after=True, delete_pool_after=True)
    return parser.parse_args(argv)


def build_executor(
    args: argparse.Namespace, *, max_autoscale_nodes: int | None = None
) -> AzureBatchExecutor:
    """Create the Azure executor configured by the smoke-test command line."""
    print(args)
    return AzureBatchExecutor(
        base_name=args.base_name,
        registry_server=args.registry_server,
        image_name=args.image_name,
        image_tag=args.image_tag,
        max_autoscale_nodes=(
            max_autoscale_nodes
            if max_autoscale_nodes is not None
            else args.max_autoscale_nodes
        ),
        chunk_size=args.chunk_size,
        delete_job_after=args.delete_job_after,
        delete_pool_after=args.delete_pool_after,
        build_image=args.build_image,
        upload_image=args.upload_image,
        image_dockerfile=args.image_dockerfile,
        poll_interval=args.poll_interval,
        max_wait=args.max_wait,
    )


def run_smoke_study(args: argparse.Namespace) -> dict[str, Any]:
    """Run two independent Azure smoke scenarios and return ordered results."""

    scenarios = (
        CalibrationScenario("baseline", {"target_data": 5.0}),
        CalibrationScenario("higher-target", {"target_data": 6.0}),
    )
    study = CalibrationStudy(
        scenarios=scenarios,
        sampler_factory=lambda scenario: build_smoke_sampler(
            particle_count=args.particle_count,
            target_data=float(scenario.parameters["target_data"]),
        ),
        cloud_executor=build_executor(
            args,
            max_autoscale_nodes=max(
                args.max_autoscale_nodes, args.max_concurrent_scenarios
            ),
        ),
        max_concurrent_scenarios=args.max_concurrent_scenarios,
        detail_log_dir=args.detail_log_dir,
        study_name=args.base_name,
    )
    return study.run()


def main(argv: Sequence[str] | None = None) -> int:
    """Run the example-model Azure smoke test and print its concise outcome."""

    args = parse_args(argv)
    if args.study:
        results_by_scenario = run_smoke_study(args)
        print(
            "Azure concurrent smoke study completed: "
            f"{', '.join(results_by_scenario)}. "
            f"Detail logs: {args.detail_log_dir}"
        )
        return 0

    sampler = build_smoke_sampler(args.particle_count)
    print(args)
    results = sampler.run(
        execution="azure_batch", cloud_executor=build_executor(args)
    )
    print(
        "Azure smoke test completed: "
        f"{results.smc_step_successes[0]} accepted particles in "
        f"{results.smc_step_attempts[0]} attempts."
    )
    return 0

if __name__ == "__main__":
    raise SystemExit(main())

