use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Attribute, Cell, Table};
use owo_colors::OwoColorize;
use percli_core::{EngineSnapshot, POS_SCALE};

use percli_core::scenario::runner::DeltaSnapshot;

pub fn print_snapshot(snap: &EngineSnapshot) {
    print_vault_table(snap);
    println!();
    if !snap.accounts.is_empty() {
        print_accounts_table(snap);
    }
}

pub fn print_delta(delta: &DeltaSnapshot) {
    if delta.vault_delta != 0 {
        println!("    vault {}", format_signed_delta(delta.vault_delta));
    }
    if delta.insurance_delta != 0 {
        println!(
            "    insurance {}",
            format_signed_delta(delta.insurance_delta)
        );
    }
    for ad in &delta.account_deltas {
        let parts: Vec<String> = [
            (ad.capital_delta != 0)
                .then(|| format!("capital {}", format_signed_delta(ad.capital_delta))),
            (ad.equity_maint_delta != 0)
                .then(|| format!("equity(M) {}", format_signed_delta(ad.equity_maint_delta))),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !parts.is_empty() {
            println!("    {} {}", ad.name, parts.join(", "));
        }
    }
}

fn format_signed_delta(val: i128) -> String {
    if val > 0 {
        format!("+{}", format_i128(val))
            .if_supports_color(owo_colors::Stream::Stdout, |t| t.green())
            .to_string()
    } else {
        format_i128(val)
            .if_supports_color(owo_colors::Stream::Stdout, |t| t.red())
            .to_string()
    }
}

fn print_vault_table(snap: &EngineSnapshot) {
    let h = snap.haircut_ratio_f64();
    let h_str = if h >= 1.0 {
        "1.000 (no stress)".to_string()
    } else {
        format!("{h:.4} (stressed)")
            .if_supports_color(owo_colors::Stream::Stdout, |t| t.yellow())
            .to_string()
    };

    let conservation_str = if snap.conservation {
        "PASS"
            .if_supports_color(owo_colors::Stream::Stdout, |t| t.green())
            .to_string()
    } else {
        "FAIL"
            .if_supports_color(owo_colors::Stream::Stdout, |t| t.red())
            .to_string()
    };

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Vault Summary").add_attribute(Attribute::Bold),
            Cell::new(""),
        ]);

    table.add_row(vec!["Vault Balance", &format_u128(snap.vault)]);
    table.add_row(vec!["Insurance Fund", &format_u128(snap.insurance_fund)]);
    table.add_row(vec!["Haircut Ratio (h)", &h_str]);
    table.add_row(vec!["Conservation", &conservation_str]);
    table.add_row(vec!["Oracle Price", &snap.last_oracle_price.to_string()]);
    table.add_row(vec!["Slot", &snap.current_slot.to_string()]);
    table.add_row(vec!["OI Long (q)", &format_pos_q(snap.oi_long_q as i128)]);
    table.add_row(vec!["OI Short (q)", &format_pos_q(snap.oi_short_q as i128)]);
    table.add_row(vec!["Funding Rate", &snap.last_funding_rate.to_string()]);
    table.add_row(vec!["Side Mode Long", &snap.side_mode_long]);
    table.add_row(vec!["Side Mode Short", &snap.side_mode_short]);

    if snap.lifetime_liquidations > 0 {
        table.add_row(vec![
            "Lifetime Liquidations",
            &snap.lifetime_liquidations.to_string(),
        ]);
    }

    println!("{table}");
}

fn print_accounts_table(snap: &EngineSnapshot) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Account").add_attribute(Attribute::Bold),
            Cell::new("Capital"),
            Cell::new("PnL"),
            Cell::new("Reserved"),
            Cell::new("Equity(M)"),
            Cell::new("Equity(I)"),
            Cell::new("Pos(eff)"),
            Cell::new("Notional"),
            Cell::new("MM"),
            Cell::new("IM"),
        ]);

    for acct in &snap.accounts {
        let pnl_str = if acct.pnl < 0 {
            format_i128(acct.pnl)
                .if_supports_color(owo_colors::Stream::Stdout, |t| t.red())
                .to_string()
        } else if acct.pnl > 0 {
            format_i128(acct.pnl)
                .if_supports_color(owo_colors::Stream::Stdout, |t| t.green())
                .to_string()
        } else {
            format_i128(acct.pnl)
        };

        let mm_str = if acct.effective_position_q == 0 {
            "-".to_string()
        } else if acct.above_maintenance_margin {
            "OK".if_supports_color(owo_colors::Stream::Stdout, |t| t.green())
                .to_string()
        } else {
            "BELOW"
                .if_supports_color(owo_colors::Stream::Stdout, |t| t.red())
                .to_string()
        };

        let im_str = if acct.effective_position_q == 0 {
            "-".to_string()
        } else if acct.above_initial_margin {
            "OK".if_supports_color(owo_colors::Stream::Stdout, |t| t.green())
                .to_string()
        } else {
            "BELOW"
                .if_supports_color(owo_colors::Stream::Stdout, |t| t.red())
                .to_string()
        };

        table.add_row(vec![
            acct.name.clone(),
            format_u128(acct.capital),
            pnl_str,
            format_u128(acct.reserved_pnl),
            format_i128(acct.equity_maint),
            format_i128(acct.equity_init),
            format_pos_q(acct.effective_position_q),
            format_u128(acct.notional),
            mm_str,
            im_str,
        ]);
    }

    println!("{table}");
}

fn format_u128(val: u128) -> String {
    let s = val.to_string();
    add_thousands_sep(&s)
}

fn format_i128(val: i128) -> String {
    if val < 0 {
        let s = val.unsigned_abs().to_string();
        format!("-{}", add_thousands_sep(&s))
    } else {
        let s = val.to_string();
        add_thousands_sep(&s)
    }
}

fn format_pos_q(q: i128) -> String {
    let scale = POS_SCALE as i128;
    let whole = q / scale;
    let frac = (q % scale).unsigned_abs();
    let sign = if q < 0 && whole == 0 { "-" } else { "" };
    if frac == 0 {
        format!("{sign}{whole}.0")
    } else {
        format!("{sign}{whole}.{frac:06}")
    }
}

fn add_thousands_sep(s: &str) -> String {
    let bytes: Vec<char> = s.chars().collect();
    let mut result = String::new();
    for (i, c) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(*c);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_pos_q_sign() {
        let scale = POS_SCALE as i128;
        assert_eq!(format_pos_q(0), "0.0");
        assert_eq!(format_pos_q(scale), "1.0");
        assert_eq!(format_pos_q(-scale), "-1.0");
        assert_eq!(format_pos_q(scale + scale / 2), "1.500000");
        assert_eq!(format_pos_q(-scale - scale / 2), "-1.500000");
        // fractional negative: between -1 and 0
        assert_eq!(format_pos_q(-scale / 2), "-0.500000");
        assert_eq!(format_pos_q(-1), "-0.000001");
    }

    #[test]
    fn thousands_separator() {
        assert_eq!(add_thousands_sep("0"), "0");
        assert_eq!(add_thousands_sep("1000"), "1,000");
        assert_eq!(add_thousands_sep("1000000"), "1,000,000");
    }

    #[test]
    fn format_i128_extremes() {
        assert_eq!(format_i128(0), "0");
        assert_eq!(format_i128(-1), "-1");
        assert_eq!(
            format_i128(i128::MIN),
            format!(
                "-{}",
                add_thousands_sep(&i128::MIN.unsigned_abs().to_string())
            )
        );
    }
}
