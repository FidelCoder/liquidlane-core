#![no_std]
#![no_main]

use liquidlane_scripts_shared::{
    has_input_lock_hash, has_input_type_hash, read_hash, read_u64, require_version, script_args,
    Hash, ScriptError, ScriptResult, HASH_SIZE,
};

ckb_std::entry!(program_entry);
ckb_std::default_alloc!();
liquidlane_scripts_shared::ckb_panic_handler!();

const VAULT_TYPE_OFFSET: usize = 0;
const LP_RECEIPT_TYPE_OFFSET: usize = HASH_SIZE;
const LP_LOCK_OFFSET: usize = HASH_SIZE * 2;
const CLAIM_ID_OFFSET: usize = HASH_SIZE * 3;
const ARGS_LEN: usize = HASH_SIZE * 4;
const MIN_DATA_LEN: usize = 1 + 1 + 8;

struct Args {
    vault_type: Hash,
    lp_receipt_type: Hash,
    lp_lock: Hash,
    _claim_id: Hash,
}

pub fn program_entry() -> i8 {
    match main() {
        Ok(()) => 0,
        Err(error) => error as i8,
    }
}

fn main() -> ScriptResult<()> {
    let args = Args::load()?;
    require_lp_claim_path(&args)?;
    validate_claim_data()?;
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
            lp_receipt_type: read_hash(&bytes, LP_RECEIPT_TYPE_OFFSET)?,
            lp_lock: read_hash(&bytes, LP_LOCK_OFFSET)?,
            _claim_id: read_hash(&bytes, CLAIM_ID_OFFSET)?,
        })
    }
}

fn require_lp_claim_path(args: &Args) -> ScriptResult<()> {
    if has_input_lock_hash(&args.lp_lock)
        && has_input_type_hash(&args.lp_receipt_type)
        && has_input_type_hash(&args.vault_type)
    {
        return Ok(());
    }
    Err(ScriptError::Unauthorized)
}

fn validate_claim_data() -> ScriptResult<()> {
    let input =
        ckb_std::high_level::load_cell_data(0, ckb_std::ckb_constants::Source::GroupInput).ok();
    let output =
        ckb_std::high_level::load_cell_data(0, ckb_std::ckb_constants::Source::GroupOutput).ok();

    if let Some(data) = input.as_ref() {
        validate_data(data)?;
    }
    if let Some(data) = output.as_ref() {
        validate_data(data)?;
    }
    Ok(())
}

fn validate_data(data: &[u8]) -> ScriptResult<()> {
    if data.len() < MIN_DATA_LEN {
        return Err(ScriptError::InvalidData);
    }
    require_version(data)?;
    let status = data[1];
    let amount = read_u64(data, 2)?;
    if status > 2 || amount == 0 {
        return Err(ScriptError::InvalidData);
    }
    Ok(())
}
