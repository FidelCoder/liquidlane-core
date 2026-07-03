#![no_std]
#![no_main]

use ckb_std::ckb_constants::Source;
use liquidlane_scripts_shared::{
    checked_sum, has_input_lock_hash, has_input_or_output_type_code_hash,
    has_input_or_output_type_hash, read_hash, read_u64, require_exact_data, script_args,
    single_group_data, Hash, ScriptError, ScriptResult, HASH_SIZE,
};

ckb_std::entry!(program_entry);
ckb_std::default_alloc!();
liquidlane_scripts_shared::ckb_panic_handler!();

const VAULT_TYPE_OFFSET: usize = 0;
const LP_LOCK_OFFSET: usize = HASH_SIZE;
const REQUEST_TYPE_OFFSET: usize = HASH_SIZE * 2;
const FEE_CLAIM_TYPE_OFFSET: usize = HASH_SIZE * 3;
const ASSET_ID_OFFSET: usize = HASH_SIZE * 4;
const POSITION_ID_OFFSET: usize = HASH_SIZE * 5;
const ARGS_LEN: usize = HASH_SIZE * 6;
const DATA_LEN: usize = 1 + 5 * 8;

#[derive(Clone, Copy)]
struct ReceiptData {
    supplied: u64,
    available: u64,
    reserved: u64,
    deployed: u64,
    claimed: u64,
}

struct Args {
    vault_type: Hash,
    lp_lock: Hash,
    request_type: Hash,
    fee_claim_type: Hash,
    _asset_id: Hash,
    _position_id: Hash,
}

struct Auth {
    lp: bool,
    vault: bool,
    request: bool,
    claim: bool,
}

pub fn program_entry() -> i8 {
    match main() {
        Ok(()) => 0,
        Err(error) => error as i8,
    }
}

fn main() -> ScriptResult<()> {
    let args = Args::load()?;
    let input = single_group_data(Source::GroupInput)?.map(|data| ReceiptData::parse(&data));
    let output = single_group_data(Source::GroupOutput)?.map(|data| ReceiptData::parse(&data));
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
            lp_lock: read_hash(&bytes, LP_LOCK_OFFSET)?,
            request_type: read_hash(&bytes, REQUEST_TYPE_OFFSET)?,
            fee_claim_type: read_hash(&bytes, FEE_CLAIM_TYPE_OFFSET)?,
            _asset_id: read_hash(&bytes, ASSET_ID_OFFSET)?,
            _position_id: read_hash(&bytes, POSITION_ID_OFFSET)?,
        })
    }
}

impl ReceiptData {
    fn parse(data: &[u8]) -> ScriptResult<Self> {
        require_exact_data(data, DATA_LEN)?;
        let receipt = Self {
            supplied: read_u64(data, 1)?,
            available: read_u64(data, 9)?,
            reserved: read_u64(data, 17)?,
            deployed: read_u64(data, 25)?,
            claimed: read_u64(data, 33)?,
        };
        if checked_sum(&[receipt.available, receipt.reserved, receipt.deployed])?
            != receipt.supplied
        {
            return Err(ScriptError::InvalidData);
        }
        Ok(receipt)
    }

    fn locked(&self) -> u64 {
        self.reserved.saturating_add(self.deployed)
    }
}

impl Auth {
    fn load(args: &Args) -> Self {
        Self {
            lp: has_input_lock_hash(&args.lp_lock),
            vault: has_input_or_output_type_hash(&args.vault_type),
            request: has_input_or_output_type_code_hash(&args.request_type),
            claim: has_input_or_output_type_code_hash(&args.fee_claim_type),
        }
    }
}

fn validate_transition(
    args: &Args,
    input: Option<ReceiptData>,
    output: Option<ReceiptData>,
) -> ScriptResult<()> {
    let auth = Auth::load(args);
    match (input, output) {
        (None, None) => Err(ScriptError::BadTransition),
        (None, Some(out)) => validate_create(&auth, &out),
        (Some(inp), None) => validate_burn(&auth, &inp),
        (Some(inp), Some(out)) => validate_update(&auth, &inp, &out),
    }
}

fn validate_create(auth: &Auth, output: &ReceiptData) -> ScriptResult<()> {
    if auth.lp && auth.vault && output.supplied > 0 && output.available == output.supplied {
        return Ok(());
    }
    Err(ScriptError::Unauthorized)
}

fn validate_burn(auth: &Auth, input: &ReceiptData) -> ScriptResult<()> {
    if auth.lp && auth.vault && input.locked() == 0 {
        return Ok(());
    }
    Err(ScriptError::BadTransition)
}

fn validate_update(auth: &Auth, input: &ReceiptData, output: &ReceiptData) -> ScriptResult<()> {
    if auth.lp && auth.vault && auth.claim {
        return require_claim_delta(input, output);
    }
    if auth.vault && auth.request {
        return require_request_delta(input, output);
    }
    if auth.lp && !auth.request {
        return require_lp_delta(input, output);
    }
    Err(ScriptError::Unauthorized)
}

fn require_lp_delta(input: &ReceiptData, output: &ReceiptData) -> ScriptResult<()> {
    if input.reserved != output.reserved
        || input.deployed != output.deployed
        || input.claimed != output.claimed
        || output.supplied < output.locked()
    {
        return Err(ScriptError::BadTransition);
    }
    Ok(())
}

fn require_request_delta(input: &ReceiptData, output: &ReceiptData) -> ScriptResult<()> {
    if input.supplied != output.supplied || input.claimed != output.claimed {
        return Err(ScriptError::BadTransition);
    }
    Ok(())
}

fn require_claim_delta(input: &ReceiptData, output: &ReceiptData) -> ScriptResult<()> {
    if input.supplied != output.supplied
        || input.available != output.available
        || input.reserved != output.reserved
        || input.deployed != output.deployed
        || output.claimed < input.claimed
    {
        return Err(ScriptError::BadTransition);
    }
    Ok(())
}
