use anyhow::{Result, anyhow};
use serde_json::Value;

use super::{
    AppStore,
    chain_settlement::{
        ARG_SEGMENT_CHARS, join_hex, next_receipt_cell, next_vault_cell, padded_id,
        previous_receipt_cell, previous_vault_cell, require_vault_fee_delta, single,
        vault_lock_script,
    },
    chain_types::{
        ChainOutput, ChainScript, outputs, parse_fee_claim_data, parse_receipt_data, required_hash,
        script_from_address, script_hash, type_code_matches,
    },
};
use crate::domain::{FeeClaim, LpPosition, User};

const FEE_CLAIM_ARGS_LEN: usize = 2 + ARG_SEGMENT_CHARS * 4;

impl AppStore {
    pub(super) async fn verify_fee_claim_tx(
        &self,
        tx_hash: &str,
        claim: &FeeClaim,
        position: &LpPosition,
        user: &User,
        signed_tx: &Option<Value>,
    ) -> Result<()> {
        self.verify_ckb_settlement_tx(tx_hash, signed_tx).await?;
        let Some(client) = self.ckb_rpc.as_ref() else {
            return Ok(());
        };
        let transaction = client.transaction_details(tx_hash).await?.transaction;
        let user_lock = script_from_address(&user.ckb_address)?;
        let vault_lock = vault_lock_script(&self.vault)?;
        let vault_type_code = required_hash(
            self.vault.scripts.vault_type_code_hash.as_deref(),
            "LIQUIDLANE_VAULT_TYPE_CODE_HASH",
        )?;
        let receipt_type_code = required_hash(
            self.vault.scripts.lp_receipt_type_code_hash.as_deref(),
            "LIQUIDLANE_LP_RECEIPT_TYPE_CODE_HASH",
        )?;
        let claim_type_code = required_hash(
            self.vault.scripts.fee_claim_type_code_hash.as_deref(),
            "LIQUIDLANE_FEE_CLAIM_TYPE_CODE_HASH",
        )?;
        let previous_vault =
            previous_vault_cell(client, &transaction, &vault_lock, &vault_type_code).await?;
        let next_vault = next_vault_cell(&transaction, &vault_lock, &vault_type_code)?;
        require_vault_fee_delta(&previous_vault, &next_vault, claim.amount)?;
        let previous_receipt = previous_receipt_cell(
            client,
            &transaction,
            position,
            &user_lock,
            &receipt_type_code,
        )
        .await?;
        let next_receipt = next_receipt_cell(&transaction, &previous_receipt)?;
        require_fee_receipt_delta(&previous_receipt, &next_receipt, claim.amount)?;
        let vault_type = next_vault
            .type_script
            .as_ref()
            .ok_or_else(|| anyhow!("vault output type script is missing"))?;
        let receipt_type = previous_receipt
            .type_script
            .as_ref()
            .ok_or_else(|| anyhow!("LP receipt type script is missing"))?;
        let claim_output = fee_claim_cell(&transaction, &user_lock, &claim_type_code)?;
        require_fee_claim_identity(&claim_output, vault_type, receipt_type, &user_lock, claim)?;
        require_fee_claim_data(&claim_output, claim.amount)
    }
}

fn fee_claim_cell(
    transaction: &Value,
    user_lock: &ChainScript,
    claim_type_code: &str,
) -> Result<ChainOutput> {
    single(
        outputs(transaction)?
            .into_iter()
            .filter(|output| {
                output.lock == *user_lock && type_code_matches(&output.type_script, claim_type_code)
            })
            .collect(),
        "fee claim transaction did not create a fee claim cell",
        "fee claim transaction created duplicate fee claim cells",
    )
}

fn require_fee_receipt_delta(
    previous: &ChainOutput,
    next: &ChainOutput,
    amount: u64,
) -> Result<()> {
    let before = parse_receipt_data(&previous.data)?;
    let after = parse_receipt_data(&next.data)?;
    if after.supplied == before.supplied
        && after.available == before.available
        && after.reserved == before.reserved
        && after.deployed == before.deployed
        && after.claimed == before.claimed.saturating_add(amount)
    {
        return Ok(());
    }
    Err(anyhow!("fee claim transaction LP receipt delta is invalid"))
}

fn require_fee_claim_identity(
    output: &ChainOutput,
    vault_type: &ChainScript,
    receipt_type: &ChainScript,
    user_lock: &ChainScript,
    claim: &FeeClaim,
) -> Result<()> {
    let Some(type_script) = output.type_script.as_ref() else {
        return Err(anyhow!("fee claim type script is missing"));
    };
    let expected_args = join_hex(&[
        script_hash(vault_type)?,
        script_hash(receipt_type)?,
        script_hash(user_lock)?,
        padded_id(&claim.id),
    ]);
    if type_script.hash_type == "data1"
        && type_script.args.len() == FEE_CLAIM_ARGS_LEN
        && type_script.args == expected_args
    {
        return Ok(());
    }
    Err(anyhow!("fee claim type args do not match the claim intent"))
}

fn require_fee_claim_data(output: &ChainOutput, amount: u64) -> Result<()> {
    let data = parse_fee_claim_data(&output.data)?;
    if (data.status == 0 || data.status == 1) && data.amount == amount {
        return Ok(());
    }
    Err(anyhow!(
        "fee claim cell data does not match the claim intent"
    ))
}
