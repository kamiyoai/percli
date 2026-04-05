use anyhow::{Context, Result};
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};
use std::str::FromStr;

const DEFAULT_PROGRAM_ID: &str = "PercQhVBxXnVCaAhfrPZFc2dVZcQANnwEYroogLJFwm";
const DEFAULT_RPC_URL: &str = "https://api.devnet.solana.com";

pub struct ChainConfig {
    pub rpc_url: String,
    pub keypair: Keypair,
    pub program_id: Pubkey,
}

impl ChainConfig {
    pub fn new(
        rpc_url: Option<&str>,
        keypair_path: Option<&str>,
        program_id: Option<&str>,
    ) -> Result<Self> {
        let rpc_url = match rpc_url {
            Some("devnet") => DEFAULT_RPC_URL.to_string(),
            Some("mainnet") => "https://api.mainnet-beta.solana.com".to_string(),
            Some("localhost" | "local") => "http://127.0.0.1:8899".to_string(),
            Some(url) => url.to_string(),
            None => DEFAULT_RPC_URL.to_string(),
        };

        let keypair_path = keypair_path.map(|s| s.to_string()).unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_default();
            format!("{home}/.config/solana/id.json")
        });

        let keypair_bytes = std::fs::read(&keypair_path)
            .with_context(|| format!("Failed to read keypair from {keypair_path}"))?;
        let keypair_json: Vec<u8> = serde_json::from_slice(&keypair_bytes)
            .with_context(|| "Invalid keypair JSON format")?;
        let keypair = Keypair::try_from(keypair_json.as_slice())
            .map_err(|e| anyhow::anyhow!("Invalid keypair bytes: {e}"))?;

        let program_id = match program_id {
            Some(id) => {
                Pubkey::from_str(id).with_context(|| format!("Invalid program ID: {id}"))?
            }
            None => Pubkey::from_str(DEFAULT_PROGRAM_ID).unwrap(),
        };

        Ok(Self {
            rpc_url,
            keypair,
            program_id,
        })
    }

    pub fn authority(&self) -> Pubkey {
        self.keypair.pubkey()
    }

    pub fn market_pda(&self) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[b"market", self.authority().as_ref()], &self.program_id)
    }
}
