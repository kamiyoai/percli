use anyhow::Result;

use crate::format::status;

pub fn run(template: &str, output: Option<&str>) -> Result<()> {
    let content = match template {
        "basic" => BASIC_TEMPLATE,
        "liquidation" => LIQUIDATION_TEMPLATE,
        "haircut" => HAIRCUT_TEMPLATE,
        other => anyhow::bail!("unknown template: {other}. Available: basic, liquidation, haircut"),
    };

    if let Some(path) = output {
        std::fs::write(path, content)?;
        status::status("Writing", &format!("{path} ({template} template)"));
        status::status_cyan("Hint", &format!("percli sim {path}"));
    } else {
        print!("{content}");
    }

    Ok(())
}

const BASIC_TEMPLATE: &str = r#"# Percolator Risk Engine — Basic Scenario
# Run with: percli sim scenario.toml

[meta]
name = "Basic Two-Party Trade"
description = "Alice goes long, Bob goes short. Price moves. Observe equity changes."

[params]
warmup_period_slots = 100
maintenance_margin_bps = 500    # 5%
initial_margin_bps = 1000       # 10%
trading_fee_bps = 0
max_accounts = 64

[market]
initial_oracle_price = 1000
initial_slot = 0

# --- Deposits ---

[[steps]]
action = "deposit"
account = "alice"
amount = 100_000

[[steps]]
action = "deposit"
account = "bob"
amount = 100_000

# --- Crank to initialize market state ---

[[steps]]
action = "crank"
oracle_price = 1000
slot = 1

# --- Trade: alice long, bob short ---

[[steps]]
action = "trade"
long = "alice"
short = "bob"
size = 50
price = 1000

# --- Price moves up 10% ---

[[steps]]
action = "crank"
oracle_price = 1100
slot = 200
comment = "Price rises 10% — alice in profit"

# --- Query final state ---

[[steps]]
action = "query"
metric = "summary"

# --- Verify conservation ---

[[steps]]
action = "assert"
condition = "conservation"
"#;

const LIQUIDATION_TEMPLATE: &str = r#"# Percolator Risk Engine — Liquidation Scenario
# Demonstrates the liquidation mechanism

[meta]
name = "Liquidation Cascade"
description = "Extreme price move triggers liquidation. Observe insurance fund and haircut."

[params]
warmup_period_slots = 100
maintenance_margin_bps = 500
initial_margin_bps = 1000
trading_fee_bps = 0
liquidation_fee_bps = 100
max_accounts = 64

[market]
initial_oracle_price = 1000
initial_slot = 0

[[steps]]
action = "deposit"
account = "alice"
amount = 100_000

[[steps]]
action = "deposit"
account = "bob"
amount = 100_000

[[steps]]
action = "crank"
oracle_price = 1000
slot = 1

[[steps]]
action = "trade"
long = "alice"
short = "bob"
size = 80
price = 1000
comment = "High leverage trade"

[[steps]]
action = "crank"
oracle_price = 500
slot = 200
comment = "Price crashes 50% — alice underwater"

[[steps]]
action = "query"
metric = "summary"
comment = "Observe alice below maintenance margin"

[[steps]]
action = "liquidate"
account = "alice"

[[steps]]
action = "query"
metric = "summary"
comment = "Post-liquidation state"

[[steps]]
action = "assert"
condition = "conservation"
"#;

const HAIRCUT_TEMPLATE: &str = r#"# Percolator Risk Engine — Haircut Stress Test
# Shows the haircut mechanism activating under vault stress

[meta]
name = "Haircut Stress Test"
description = "Multiple traders, extreme losses, observe how haircut socializes losses fairly."

[params]
warmup_period_slots = 10
maintenance_margin_bps = 500
initial_margin_bps = 1000
trading_fee_bps = 0
max_accounts = 64

[market]
initial_oracle_price = 1000
initial_slot = 0

[[steps]]
action = "deposit"
account = "alice"
amount = 500_000

[[steps]]
action = "deposit"
account = "bob"
amount = 500_000

[[steps]]
action = "deposit"
account = "charlie"
amount = 200_000

[[steps]]
action = "crank"
oracle_price = 1000
slot = 1

[[steps]]
action = "trade"
long = "alice"
short = "bob"
size = 400
price = 1000
comment = "Large leveraged position"

[[steps]]
action = "crank"
oracle_price = 500
slot = 200
comment = "Price crashes 50%"

[[steps]]
action = "query"
metric = "summary"
comment = "Observe haircut ratio and account states"

[[steps]]
action = "assert"
condition = "conservation"
comment = "Conservation MUST hold even under extreme stress"
"#;
