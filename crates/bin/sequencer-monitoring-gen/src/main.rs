use std::path::PathBuf;
use clap::{Arg, Command};
use std::fs::OpenOptions;
use std::io::{self, Write};


const DUMMY_JSON: &str = r#"{
  "name": "dashboarad_example_1",
  "description": "dashboarad_default_description_2",
  "rows": [
    {
      "name": "row_example_1",
      "description": "row_default_description_1",
      "panels": [
        {
          "name": "row_example_1",
          "description": "panel_default_description_1",
          "expr": "expr1",
          "panel_type": "Stat"
        },
        {
          "name": "row_example_2",
          "description": "panel_default_description_2",
          "expr": "expr2",
          "panel_type": "Stat"
        }
      ]
    },
    {
      "name": "row_example_2",
      "description": "row_default_description_2",
      "panels": [
        {
          "name": "row_example_3",
          "description": "panel_default_description_3",
          "expr": "expr3",
          "panel_type": "Stat"
        },
        {
          "name": "row_example_4",
          "description": "panel_default_description_4",
          "expr": "expr4",
          "panel_type": "Stat"
        }
      ]
    }
  ]
}"#;


fn main() -> io::Result<()> {
    let matches = Command::new("sequencer-monitoring-gen")
    .arg(
        Arg::new("output")
        .short('o')
        .long("output")
        .help("The output file path"),
    ).get_matches();

    let output: PathBuf = matches.get_one::<String>("output").unwrap().into();
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(&output)?;

    // Write more text to the file
    file.write_all(&DUMMY_JSON.as_bytes())?;

    println!("Results writen to {:?}", output);

    Ok(())
}
