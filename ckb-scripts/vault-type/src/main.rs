#![no_std]
#![no_main]

use ckb_std::ckb_constants::Source;
use liquidlane_scripts_shared::{
    current_script_hash, has_input_lock_hash, has_input_type_hash, matching_capacity,
    matching_data, read_hash, read_u64, require_version, script_args, Hash, ScriptError,
    ScriptResult, HASH_SIZE,
};

ckb_std::entry!(program_entry);
ckb_std::default_alloc!();
liquidlane_scripts_shared::ckb_panic_handler!();

const ADMIN_OFFSET: usize = 0;
const LP_RECEIPT_OFFSET: usize = HASH_SIZE;
const REQUEST_OFFSET: usize = HASH_SIZE * 2;
const FEE_CLAIM_OFFSET: usize = HASH_SIZE * 3;
const ARGS_LEN: usize = HASH_SIZE * 4;
const MIN_DATA_LEN: usize = 1 + 4 * 8;

struct Args {
    admin_lock: Hash,
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
    let self_hash = current_script_hash()?;
    validate_data_cells(&self_hash)?;
    require_authorized_transition(&args)?;
    require_non_decreasing_capacity(&self_hash, &args)?;
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
            lp_receipt_type: read_hash(&bytes, LP_RECEIPT_OFFSET)?,
            request_type: read_hash(&bytes, REQUEST_OFFSET)?,
            fee_claim_type: read_hash(&bytes, FEE_CLAIM_OFFSET)?,
        })
    }
}

fn validate_data_cells(self_hash: &Hash) -> ScriptResult<()> {
    for data in matching_data(self_hash, Source::Input)
        .into_iter()
        .chain(matching_data(self_hash, Source::Output))
    {
        if data.len() < MIN_DATA_LEN {
            return Err(ScriptError::InvalidData);
        }
        require_version(&data)?;
        let total = read_u64(&data, 1)?;
        let reserved = read_u64(&data, 9)?;
        let deployed = read_u64(&data, 17)?;
        if reserved.saturating_add(deployed) > total {
            return Err(ScriptError::InvalidData);
        }
    }
    Ok(())
}

fn require_authorized_transition(args: &Args) -> ScriptResult<()> {
    if has_input_lock_hash(&args.admin_lock)
        || has_input_type_hash(&args.lp_receipt_type)
        || has_input_type_hash(&args.request_type)
        || has_input_type_hash(&args.fee_claim_type)
    {
        return Ok(());
    }
    Err(ScriptError::Unauthorized)
}

fn require_non_decreasing_capacity(self_hash: &Hash, args: &Args) -> ScriptResult<()> {
    let input_capacity = matching_capacity(self_hash, Source::Input);
    let output_capacity = matching_capacity(self_hash, Source::Output);
    if output_capacity >= input_capacity || has_input_lock_hash(&args.admin_lock) {
        return Ok(());
    }
    if has_input_type_hash(&args.lp_receipt_type) || has_input_type_hash(&args.fee_claim_type) {
        return Ok(());
    }
    Err(ScriptError::ValueMismatch)
}
