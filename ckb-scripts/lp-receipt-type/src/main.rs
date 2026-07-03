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
const LP_LOCK_OFFSET: usize = HASH_SIZE;
const ASSET_ID_OFFSET: usize = HASH_SIZE * 2;
const POSITION_ID_OFFSET: usize = HASH_SIZE * 3;
const ARGS_LEN: usize = HASH_SIZE * 4;
const MIN_DATA_LEN: usize = 1 + 5 * 8;

struct Args {
    vault_type: Hash,
    lp_lock: Hash,
    _asset_id: Hash,
    _position_id: Hash,
}

pub fn program_entry() -> i8 {
    match main() {
        Ok(()) => 0,
        Err(error) => error as i8,
    }
}

fn main() -> ScriptResult<()> {
    let args = Args::load()?;
    require_lp_or_vault_path(&args)?;
    validate_group_data()?;
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
            lp_lock: read_hash(&bytes, LP_LOCK_OFFSET)?,
            _asset_id: read_hash(&bytes, ASSET_ID_OFFSET)?,
            _position_id: read_hash(&bytes, POSITION_ID_OFFSET)?,
        })
    }
}

fn require_lp_or_vault_path(args: &Args) -> ScriptResult<()> {
    if has_input_lock_hash(&args.lp_lock) || has_output_type_hash(&args.vault_type) {
        return Ok(());
    }
    Err(ScriptError::Unauthorized)
}

fn validate_group_data() -> ScriptResult<()> {
    let input =
        ckb_std::high_level::load_cell_data(0, ckb_std::ckb_constants::Source::GroupInput).ok();
    let output =
        ckb_std::high_level::load_cell_data(0, ckb_std::ckb_constants::Source::GroupOutput).ok();

    if let Some(data) = input.as_ref() {
        validate_receipt_data(data)?;
    }
    if let Some(data) = output.as_ref() {
        validate_receipt_data(data)?;
    }
    if input.is_some() && output.is_none() {
        return Err(ScriptError::BadTransition);
    }
    Ok(())
}

fn validate_receipt_data(data: &[u8]) -> ScriptResult<()> {
    if data.len() < MIN_DATA_LEN {
        return Err(ScriptError::InvalidData);
    }
    require_version(data)?;
    let supplied = read_u64(data, 1)?;
    let available = read_u64(data, 9)?;
    let reserved = read_u64(data, 17)?;
    let deployed = read_u64(data, 25)?;
    let claimed = read_u64(data, 33)?;
    if available.saturating_add(reserved).saturating_add(deployed) > supplied {
        return Err(ScriptError::InvalidData);
    }
    if claimed > supplied {
        return Err(ScriptError::InvalidData);
    }
    Ok(())
}
