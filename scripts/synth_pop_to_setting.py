import polars as pl
import argparse

def main(input_file: str, output_file: str) -> None:
    df = (
        pl.read_csv(input_file)
        .with_columns(
            pl.all().cast(pl.Utf8),
            pl.col("homeId").cast(pl.Utf8).str.slice(0, 11).alias("censustractId"),
        )
        .drop("age")
        .unpivot(variable_name="setting_category", value_name="setting_code")
        .unique()
        .drop_nulls()
    )
    df.write_csv(output_file)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Convert synthetic population to settings")
    parser.add_argument("--input", default="input/people_test.csv", help="Input CSV file")
    parser.add_argument("--output", default="input/synth_pop_to_settings.csv", help="Output CSV file")
    args = parser.parse_args()
    
    main(args.input, args.output)