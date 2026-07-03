#![no_std]
#![no_main]

use ckb_std::ckb_constants::Source;
use liquidlane_scripts_shared::{
    has_input_lock_hash, has_input_or_output_type_hash, read_hash, read_u64, require_exact_data,
    script_args, single_group_data, Hash, ScriptError, ScriptResult, HASH_SIZE,
};

ckb_std::entry!(program_entry);
ckb_std::default_alloc!();
liquidlane_scripts_shared::ckb_panic_handler!();

const VAULT_TYPE_OFFSET: usize = 0;
const LP_RECEIPT_TYPE_OFFSET: usize = HASH_SIZE;
const LP_LOCK_OFFSET: usize = HASH_SIZE * 2;
const CLAIM_ID_OFFSET: usize = HASH_SIZE * 3;
const ARGS_LEN: usize = HASH_SIZE * 4;
const DATA_LEN: usize = 1 + 1 + 8;
const STATUS_PENDING: u8 = 0;
const STATUS_APPROVED: u8 = 1;
const STATUS_PAID: u8 = 2;

#[derive(Clone, Copy)]
struct ClaimData {
    status: u8,
    amount: u64,
}

struct Args {
    vault_type: Hash,
    lp_receipt_type: Hash,
    lp_lock: Hash,
    _claim_id: Hash,
}

struct Auth {
    lp: bool,
    vault: bool,
    receipt: bool,
}

pub fn program_entry() -> i8 {
    match main() {
        Ok(()) => 0,
        Err(error) => error as i8,
    }
}

fn main() -> ScriptResult<()> {
    let args = Args::load()?;
    let input = single_group_data(Source::GroupInput)?.map(|data| ClaimData::parse(&data));
    let output = single_group_data(Source::GroupOutput)?.map(|data| ClaimData::parse(&data));
    validate_transition(&args, input.transpose()?, output.transpose()?)
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

impl ClaimData {
    fn parse(data: &[u8]) -> ScriptResult<Self> {
        require_exact_data(data, DATA_LEN)?;
        let claim = Self {
            status: data[1],
            amount: read_u64(data, 2)?,
        };
        if claim.status > STATUS_PAID || claim.amount == 0 {
            return Err(ScriptError::InvalidData);
        }
        Ok(claim)
    }
}

impl Auth {
    fn load(args: &Args) -> Self {
        Self {
            lp: has_input_lock_hash(&args.lp_lock),
            vault: has_input_or_output_type_hash(&args.vault_type),
            receipt: has_input_or_output_type_hash(&args.lp_receipt_type),
        }
    }

    fn allowed(&self) -> bool {
        self.lp && self.vault && self.receipt
    }
}

fn validate_transition(
    args: &Args,
    input: Option<ClaimData>,
    output: Option<ClaimData>,
) -> ScriptResult<()> {
    let auth = Auth::load(args);
    if !auth.allowed() {
        return Err(ScriptError::Unauthorized);
    }
    match (input, output) {
        (None, None) => Err(ScriptError::BadTransition),
        (None, Some(out)) => validate_create(&out),
        (Some(inp), None) => validate_delete(&inp),
        (Some(inp), Some(out)) => validate_update(&inp, &out),
    }
}

fn validate_create(output: &ClaimData) -> ScriptResult<()> {
    if output.status == STATUS_PENDING || output.status == STATUS_APPROVED {
        return Ok(());
    }
    Err(ScriptError::BadTransition)
}

fn validate_delete(input: &ClaimData) -> ScriptResult<()> {
    if input.status == STATUS_PAID {
        return Ok(());
    }
    Err(ScriptError::BadTransition)
}

fn validate_update(input: &ClaimData, output: &ClaimData) -> ScriptResult<()> {
    if input.amount != output.amount || output.status < input.status {
        return Err(ScriptError::BadTransition);
    }
    Ok(())
}
