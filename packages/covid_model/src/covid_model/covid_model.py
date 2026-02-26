from typing import Any

from mrp import MRPModel
from pathlib import Path
import polars as pl
import json
import subprocess

class Covid_Ixa_Model(MRPModel):
    def run(self):
        results = self.simulate(self.input)
        return results
        
    @staticmethod
    def simulate(inputs_dict: dict[str, Any]) -> pl.DataFrame:
        model_inputs = inputs_dict["model_inputs"]
        model_config = inputs_dict["model_config"]
        input_file = Path(model_config["output_dir"], "input.json")

        with open(input_file, 'w') as f:
            json.dump(model_inputs, f, indent=4)
    
        cmd = [
            model_config['exe_file'],
            "--config",
            input_file,
            "--output",
            model_config['output_dir'],
            "-f",
            "--no-stats"
        ]
        try:
            out_tmp = subprocess.run(cmd, capture_output=True, check=True)
            ##print(out_tmp.stdout)
            ##print(out_tmp.stderr)
        except subprocess.CalledProcessError as e:
            print(f"ERROR: {e}")
            print("ERROR OUTPUT:", e.stderr)
            
        return pl.read_csv(Path(model_config['output_dir'], "person_property_count.csv"))
