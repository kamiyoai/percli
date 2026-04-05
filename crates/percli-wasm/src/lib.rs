use percli_core::scenario::{runner, Scenario};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn run_scenario(toml_input: &str) -> String {
    match run_inner(toml_input) {
        Ok(json) => json,
        Err(e) => {
            let msg = format!("{e:#}");
            serde_json::json!({ "error": msg }).to_string()
        }
    }
}

fn run_inner(toml_input: &str) -> Result<String, Box<dyn std::error::Error>> {
    let scenario: Scenario = toml::from_str(toml_input)?;

    let opts = runner::RunOptions {
        check_conservation: true,
        step_by_step: true,
        verbose: false,
    };

    let result = runner::run_scenario(&scenario, &opts)?;
    let json = serde_json::to_string(&result)?;
    Ok(json)
}
