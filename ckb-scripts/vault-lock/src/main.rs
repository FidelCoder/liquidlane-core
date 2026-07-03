#![no_std]
#![no_main]

use ckb_std::ckb_constants::Source;
use liquidlane_scripts_shared::{
    count_lock_hash, current_script_hash, has_input_lock_hash, has_input_type_hash, read_hash,
    script_args, Hash, ScriptError, ScriptResult, HASH_SIZE,
};

ckb_std::entry!(program_entry);
ckb_std::default_alloc!();
liquidlane_scripts_shared::ckb_panic_handler!();

const ADMIN_OFFSET: usize = 0;
const VAULT_TYPE_OFFSET: usize = HASH_SIZE;
const LP_RECEIPT_OFFSET: usize = HASH_SIZE * 2;
const REQUEST_OFFSET: usize = HASH_SIZE * 3;
const FEE_CLAIM_OFFSET: usize = HASH_SIZE * 4;
const ARGS_LEN: usize = HASH_SIZE * 5;

struct Args {
    admin_lock: Hash,
    vault_type: Hash,
    lp_receipt_type: Hash,
    request_type: Hash,
    fee_claim_type: Hash,
}

pub fn program_entry() -> i8 {
    match main() {
        Ok(()) => 0,
        Err(error) => error as i8,
    }
}

fn main() -> ScriptResult<()> {
    let args = Args::load()?;
    require_authorized_spend(&args)?;
    require_vault_cells_remain_typed(&args)?;
    Ok(())
}

impl Args {
    fn load() -> ScriptResult<Self> {
        let bytes = script_args()?;
        if bytes.len() != ARGS_LEN {
            return Err(ScriptError::InvalidArgs);
        }
        Ok(Self {
            admin_lock: read_hash(&bytes, ADMIN_OFFSET)?,
            vault_type: read_hash(&bytes, VAULT_TYPE_OFFSET)?,
            lp_receipt_type: read_hash(&bytes, LP_RECEIPT_OFFSET)?,
            request_type: read_hash(&bytes, REQUEST_OFFSET)?,
            fee_claim_type: read_hash(&bytes, FEE_CLAIM_OFFSET)?,
        })
    }
}

fn require_authorized_spend(args: &Args) -> ScriptResult<()> {
    if has_input_lock_hash(&args.admin_lock) {
        return Ok(());
    }
    if has_input_type_hash(&args.lp_receipt_type)
        || has_input_type_hash(&args.request_type)
        || has_input_type_hash(&args.fee_claim_type)
    {
        return Ok(());
    }
    Err(ScriptError::Unauthorized)
}

fn require_vault_cells_remain_typed(args: &Args) -> ScriptResult<()> {
    let own_lock = current_script_hash()?;
    let output_vault_lock_count = count_lock_hash(&own_lock, Source::Output);
    if output_vault_lock_count == 0 {
        return Ok(());
    }
    let output_vault_type_count =
        liquidlane_scripts_shared::count_type_hash(&args.vault_type, Source::Output);
    if output_vault_type_count < output_vault_lock_count {
        return Err(ScriptError::MissingVault);
    }
    Ok(())
}
