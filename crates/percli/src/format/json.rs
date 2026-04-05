use anyhow::Result;
use percli_core::scenario::runner::{RunResult, StepOutcome};
use percli_core::EngineSnapshot;
use serde_json::{json, Value};

pub fn print_snapshot(snap: &EngineSnapshot) -> Result<()> {
    let j = serde_json::to_string_pretty(snap)?;
    println!("{j}");
    Ok(())
}

pub fn print_run_result(result: &RunResult) -> Result<()> {
    let steps: Vec<Value> = result
        .step_results
        .iter()
        .map(|sr| {
            let status = match &sr.outcome {
                StepOutcome::Ok => "ok",
                StepOutcome::Warning(_) => "warning",
                StepOutcome::QueryResult(_) => "query",
                StepOutcome::AssertPassed => "pass",
                StepOutcome::AssertFailed(_) => "fail",
            };
            let message = match &sr.outcome {
                StepOutcome::Warning(w) => Some(w.clone()),
                StepOutcome::AssertFailed(m) => Some(m.clone()),
                _ => None,
            };
            let mut step_val = json!({
                "step": sr.step_num,
                "description": sr.description,
                "status": status,
            });
            if let Some(msg) = message {
                step_val["message"] = json!(msg);
            }
            if let StepOutcome::QueryResult(snap) = &sr.outcome {
                step_val["snapshot"] = serde_json::to_value(snap).unwrap_or(json!(null));
            }
            step_val
        })
        .collect();

    let output = json!({
        "steps": steps,
        "final_state": result.final_snapshot,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
