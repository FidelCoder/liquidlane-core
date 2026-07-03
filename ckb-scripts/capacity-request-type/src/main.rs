#![no_std]
#![no_main]

use liquidlane_scripts_shared::{
    has_input_lock_hash, has_output_type_hash, read_hash, read_u64, require_version, script_args,
    Hash, ScriptError, ScriptResult, HASH_SIZE,
};

ckb_std::entry!(program_entry);
ckb_std::default_alloc!();
liquidlane_scripts_shared::ckb_panic_handler!();

const VAULT_TYPE_OFFSET: usize = 0;
const MERCHANT_LOCK_OFFSET: usize = HASH_SIZE;
const OPERATOR_LOCK_OFFSET: usize = HASH_SIZE * 2;
const REQUEST_ID_OFFSET: usize = HASH_SIZE * 3;
const ARGS_LEN: usize = HASH_SIZE * 4;
const MIN_DATA_LEN: usize = 1 + 1 + 3 * 8;

struct Args {
    vault_type: Hash,
    merchant_lock: Hash,
    operator_lock: Hash,
    _request_id: Hash,
}

pub fn program_entry() -> i8 {
    match main() {
        Ok(()) => 0,
        Err(error) => error as i8,
    }
}

fn main() -> ScriptResult<()> {
    let args = Args::load()?;
    require_actor(&args)?;
    validate_transition(&args)?;
    Ok(())
}

impl Args {
    fn load() -> ScriptResult<Self> {
        let bytes = script_args()?;
        if bytes.len() != ARGS_LEN {
            return Err(ScriptError::InvalidArgs);
        }
        Ok(Self {
            vault_type: read_hash(&bytes, VAULT_TYPE_OFFSET)?,
            merchant_lock: read_hash(&bytes, MERCHANT_LOCK_OFFSET)?,
            operator_lock: read_hash(&bytes, OPERATOR_LOCK_OFFSET)?,
            _request_id: read_hash(&bytes, REQUEST_ID_OFFSET)?,
        })
    }
}

fn require_actor(args: &Args) -> ScriptResult<()> {
    if has_input_lock_hash(&args.merchant_lock) || has_input_lock_hash(&args.operator_lock) {
        return Ok(());
    }
    Err(ScriptError::Unauthorized)
}

fn validate_transition(args: &Args) -> ScriptResult<()> {
    let input =
        ckb_std::high_level::load_cell_data(0, ckb_std::ckb_constants::Source::GroupInput).ok();
    let output =
        ckb_std::high_level::load_cell_data(0, ckb_std::ckb_constants::Source::GroupOutput).ok();

    if let Some(data) = input.as_ref() {
        validate_request_data(data)?;
    }
    if let Some(data) = output.as_ref() {
        validate_request_data(data)?;
    }
    if output.is_none() && !has_output_type_hash(&args.vault_type) {
        return Err(ScriptError::MissingVault);
    }
    Ok(())
}

fn validate_request_data(data: &[u8]) -> ScriptResult<()> {
    if data.len() < MIN_DATA_LEN {
        return Err(ScriptError::InvalidData);
    }
    require_version(data)?;
    let status = data[1];
    let amount = read_u64(data, 2)?;
    let lease_fee = read_u64(data, 10)?;
    let expiry = read_u64(data, 18)?;
    if status > 3 || amount == 0 || lease_fee == 0 || expiry == 0 {
        return Err(ScriptError::InvalidData);
    }
    Ok(())
}
