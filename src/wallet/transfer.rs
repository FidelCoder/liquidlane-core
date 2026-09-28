use crate::{
    connector::config::write_private,
    marketplace::{
        crypto,
        model::{SHANNONS, TESTNET_GENESIS},
    },
};
use anyhow::{Context, Result, ensure};
use ckb_sdk::{
    Address, CkbRpcClient, NetworkInfo, NetworkType,
    transaction::{
        TransactionBuilderConfiguration,
        builder::{CkbTransactionBuilder, SimpleTransactionBuilder},
        input::InputIterator,
        signer::{SignContexts, TransactionSigner},
    },
};
use ckb_types::{
    H256,
    core::Capacity,
    packed::{Bytes, CellOutput},
    prelude::*,
};
use std::{path::Path, str::FromStr};

pub fn send(
    key: &secp256k1::SecretKey,
    rpc_url: &str,
    recipient: &str,
    amount: u64,
    receipt: &Path,
) -> Result<()> {
    ensure!(
        !receipt.exists(),
        "receipt already exists; reconcile the stored transaction before another transfer"
    );
    crypto::address_script(recipient)?;
    let rpc = CkbRpcClient::new(rpc_url);
    let genesis = rpc
        .get_block_hash(0.into())?
        .context("genesis unavailable")?;
    ensure!(
        format!("{genesis:#x}") == TESTNET_GENESIS,
        "transfer restricted to CKB testnet"
    );
    let network = NetworkInfo::new(NetworkType::Testnet, rpc_url.into());
    let configuration = TransactionBuilderConfiguration::new_with_network(network.clone())?;
    let sender = Address::from_str(&crypto::native_address(&crypto::pubkey(key))?)
        .map_err(anyhow::Error::msg)?;
    let recipient = Address::from_str(recipient).map_err(anyhow::Error::msg)?;
    let capacity = Capacity::shannons(amount.checked_mul(SHANNONS).context("amount overflow")?);
    let output = CellOutput::new_builder()
        .lock(&recipient)
        .capacity(capacity.pack())
        .build();
    ensure!(
        capacity >= output.occupied_capacity(Capacity::zero())?,
        "amount below recipient cell minimum"
    );
    let mut builder = SimpleTransactionBuilder::new(
        configuration,
        InputIterator::new_with_address(&[sender], &network),
    );
    builder.add_output_and_data(output, Bytes::default());
    let mut transaction = builder.build(&Default::default())?;
    TransactionSigner::new(&network).sign_transaction(
        &mut transaction,
        &SignContexts::new_sighash_h256(vec![H256::from_slice(&key.secret_bytes())?])?,
    )?;
    let tx = ckb_jsonrpc_types::TransactionView::from(transaction.get_tx_view().clone());
    // Save exact signed bytes before broadcasting; timeout recovery reuses its hash.
    write_private(receipt, &tx)?;
    let hash = rpc
        .send_transaction(tx.inner, None)
        .context("submission uncertain; inspect receipt hash before retrying")?;
    println!(
        "Submitted {hash:#x}; signed transaction stored in {}",
        receipt.display()
    );
    Ok(())
}
