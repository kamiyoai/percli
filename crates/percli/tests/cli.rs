use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::NamedTempFile;

fn percli() -> Command {
    Command::cargo_bin("percli").unwrap()
}

#[test]
fn init_basic() {
    percli()
        .args(["init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[meta]"))
        .stdout(predicate::str::contains("[params]"))
        .stdout(predicate::str::contains("[[steps]]"));
}

#[test]
fn init_all_templates() {
    for template in ["basic", "liquidation", "haircut"] {
        percli()
            .args(["init", "--template", template])
            .assert()
            .success()
            .stdout(predicate::str::contains("[meta]"));
    }
}

#[test]
fn init_unknown_template() {
    percli()
        .args(["init", "--template", "nonexistent"])
        .assert()
        .failure();
}

#[test]
fn sim_all_scenarios() {
    for entry in std::fs::read_dir("../../scenarios").unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "toml") {
            percli()
                .args(["sim", path.to_str().unwrap()])
                .assert()
                .success();
        }
    }
}

#[test]
fn sim_json_valid() {
    let output = percli()
        .args([
            "sim",
            "../../scenarios/basic-trade.toml",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(parsed["final_state"].is_object());
    assert!(parsed["steps"].is_array());
}

#[test]
fn sim_step_by_step() {
    let output = percli()
        .args(["sim", "../../scenarios/basic-trade.toml", "--step-by-step"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let count = stdout.matches("Vault Summary").count();
    assert!(
        count >= 3,
        "expected multiple Vault Summary tables, got {count}"
    );
}

#[test]
fn sim_verbose() {
    percli()
        .args(["sim", "../../scenarios/basic-trade.toml", "--verbose"])
        .assert()
        .success()
        .stdout(predicate::str::contains("vault +"));
}

#[test]
fn sim_override() {
    percli()
        .args([
            "sim",
            "../../scenarios/basic-trade.toml",
            "--override",
            "maintenance_margin_bps=300",
        ])
        .assert()
        .success();
}

#[test]
fn step_deposit_creates_state() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap();
    std::fs::remove_file(path).ok();

    percli()
        .args(["step", "--state", path, "deposit", "alice", "100000"])
        .assert()
        .success();

    let content = std::fs::read_to_string(path).unwrap();
    let state: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(state["version"], 2);
    assert!(state["operations"].is_array());
}

#[test]
fn step_roundtrip() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap();
    std::fs::remove_file(path).ok();

    percli()
        .args(["step", "--state", path, "deposit", "alice", "100000"])
        .assert()
        .success();

    percli()
        .args(["step", "--state", path, "deposit", "bob", "100000"])
        .assert()
        .success();

    percli()
        .args([
            "step", "--state", path, "crank", "--oracle", "1000", "--slot", "1",
        ])
        .assert()
        .success();

    percli()
        .args(["step", "--state", path, "set-oracle", "1100"])
        .assert()
        .success();

    percli()
        .args(["query", "--state", path, "vault"])
        .assert()
        .success()
        .stdout(predicate::str::contains("200,000").or(predicate::str::contains("200000")));
}

#[test]
fn inspect_valid() {
    percli()
        .args(["inspect", "../../scenarios/basic-trade.toml"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Scenario is valid"));
}

#[test]
fn inspect_invalid() {
    let tmp = NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "this is not valid toml {{{{").unwrap();

    percli()
        .args(["inspect", tmp.path().to_str().unwrap()])
        .assert()
        .failure();
}

#[test]
fn query_all_metrics() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap();
    std::fs::remove_file(path).ok();

    // set up state with a deposit
    percli()
        .args(["step", "--state", path, "deposit", "alice", "100000"])
        .assert()
        .success();

    for metric in [
        "summary",
        "all",
        "haircut",
        "conservation",
        "vault",
        "accounts",
    ] {
        percli()
            .args(["query", "--state", path, metric])
            .assert()
            .success();
    }

    for metric in ["equity", "margin", "position"] {
        percli()
            .args(["query", "--state", path, metric, "--account", "alice"])
            .assert()
            .success();
    }
}

#[test]
fn query_json_format() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap();
    std::fs::remove_file(path).ok();

    percli()
        .args(["step", "--state", path, "deposit", "alice", "100000"])
        .assert()
        .success();

    let output = percli()
        .args(["query", "--state", path, "vault", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(parsed["vault"].is_number());
}

#[test]
fn query_unknown_metric_fails() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap();
    std::fs::remove_file(path).ok();

    percli()
        .args(["step", "--state", path, "deposit", "alice", "100000"])
        .assert()
        .success();

    percli()
        .args(["query", "--state", path, "bogus"])
        .assert()
        .failure();
}

#[test]
fn query_account_metric_without_account_fails() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap();
    std::fs::remove_file(path).ok();

    percli()
        .args(["step", "--state", path, "deposit", "alice", "100000"])
        .assert()
        .success();

    percli()
        .args(["query", "--state", path, "equity"])
        .assert()
        .failure();
}

#[test]
fn state_version_rejects_old() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap();

    std::fs::write(
        path,
        r#"{"version":1,"params":{},"oracle_price":1000,"slot":0,"funding_rate":0,"operations":[]}"#,
    )
    .unwrap();

    percli()
        .args(["step", "--state", path, "deposit", "alice", "100000"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("too old"));
}

#[test]
fn completions_bash() {
    percli()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_percli"));
}

#[test]
fn completions_zsh() {
    percli().args(["completions", "zsh"]).assert().success();
}
