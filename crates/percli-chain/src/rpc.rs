use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
    transaction::Transaction,
};

pub struct ChainRpc {
    client: RpcClient,
}

impl ChainRpc {
    pub fn new(rpc_url: &str) -> Self {
        Self {
            client: RpcClient::new_with_commitment(
                rpc_url.to_string(),
                CommitmentConfig::confirmed(),
            ),
        }
    }

    pub fn send_tx(&self, ixs: &[Instruction], payer: &Keypair) -> Result<Signature> {
        let recent_blockhash = self
            .client
            .get_latest_blockhash()
            .context("Failed to get recent blockhash")?;

        let tx = Transaction::new_signed_with_payer(
            ixs,
            Some(&payer.pubkey()),
            &[payer],
            recent_blockhash,
        );

        let sig = self
            .client
            .send_and_confirm_transaction(&tx)
            .context("Transaction failed")?;

        Ok(sig)
    }

    pub fn get_account_data(&self, pubkey: &Pubkey) -> Result<Vec<u8>> {
        let account = self
            .client
            .get_account(pubkey)
            .with_context(|| format!("Failed to fetch account {pubkey}"))?;
        Ok(account.data)
    }

    pub fn account_exists(&self, pubkey: &Pubkey) -> Result<bool> {
        match self.client.get_account(pubkey) {
            Ok(_) => Ok(true),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("AccountNotFound") || msg.contains("could not find account") {
                    Ok(false)
                } else {
                    Err(e).context("RPC error checking account")
                }
            }
        }
    }

    pub fn get_slot(&self) -> Result<u64> {
        self.client.get_slot().context("Failed to get slot")
    }
}
