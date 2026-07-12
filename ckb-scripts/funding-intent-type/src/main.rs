#![no_std]
#![no_main]

use ckb_std::ckb_constants::Source;
use liquidlane_scripts_shared::{
    HASH_SIZE, Hash, ScriptError, ScriptResult, count_lock_hash, has_input_lock_hash,
    has_input_or_output_type_code_hash, has_input_or_output_type_hash, read_hash, read_u64,
    require_exact_data, script_args, single_group_data,
};

ckb_std::entry!(program_entry);
ckb_std::default_alloc!();
liquidlane_scripts_shared::ckb_panic_handler!();

const VAULT_TYPE_OFFSET: usize = 0;
const REQUEST_TYPE_OFFSET: usize = HASH_SIZE;
const EXECUTOR_LOCK_OFFSET: usize = HASH_SIZE * 2;
const FUNDING_LOCK_OFFSET: usize = HASH_SIZE * 3;
const REQUEST_ID_OFFSET: usize = HASH_SIZE * 4;
const ARGS_LEN: usize = HASH_SIZE * 5;
const DATA_LEN: usize = 1 + 1 + 8;
const STATUS_READY: u8 = 0;
const STATUS_SUBMITTED: u8 = 1;
const STATUS_ACTIVE: u8 = 2;
const STATUS_FAILED: u8 = 3;

#[derive(Clone, Copy)]
struct FundingData {
    status: u8,
    amount: u64,
}

struct Args {
    vault_type: Hash,
    request_type: Hash,
    executor_lock: Hash,
    funding_lock: Hash,
    _request_id: Hash,
}

struct Auth {
    executor: bool,
    vault: bool,
    request: bool,
    funding_output: bool,
}

pub fn program_entry() -> i8 {
    match main() {
        Ok(()) => 0,
        Err(error) => error as i8,
    }
}

fn main() -> ScriptResult<()> {
    let args = Args::load()?;
    let input = single_group_data(Source::GroupInput)?.map(|data| FundingData::parse(&data));
    let output = single_group_data(Source::GroupOutput)?.map(|data| FundingData::parse(&data));
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
            request_type: read_hash(&bytes, REQUEST_TYPE_OFFSET)?,
            executor_lock: read_hash(&bytes, EXECUTOR_LOCK_OFFSET)?,
            funding_lock: read_hash(&bytes, FUNDING_LOCK_OFFSET)?,
            _request_id: read_hash(&bytes, REQUEST_ID_OFFSET)?,
        })
    }
}

impl FundingData {
    fn parse(data: &[u8]) -> ScriptResult<Self> {
        require_exact_data(data, DATA_LEN)?;
        let funding = Self {
            status: data[1],
            amount: read_u64(data, 2)?,
        };
        if funding.status > STATUS_FAILED || funding.amount == 0 {
            return Err(ScriptError::InvalidData);
        }
        Ok(funding)
    }
}

impl Auth {
    fn load(args: &Args) -> Self {
        Self {
            executor: has_input_lock_hash(&args.executor_lock),
            vault: has_input_or_output_type_hash(&args.vault_type),
            request: has_input_or_output_type_code_hash(&args.request_type),
            funding_output: count_lock_hash(&args.funding_lock, Source::Output) == 1,
        }
    }

    fn funding_authorized(&self) -> bool {
        self.executor && self.vault && self.request && self.funding_output
    }
}

fn validate_transition(
    args: &Args,
    input: Option<FundingData>,
    output: Option<FundingData>,
) -> ScriptResult<()> {
    let auth = Auth::load(args);
    match (input, output) {
        (None, None) => Err(ScriptError::BadTransition),
        (None, Some(out)) => validate_create(&auth, &out),
        (Some(inp), None) => validate_delete(&auth, &inp),
        (Some(inp), Some(out)) => validate_update(&auth, &inp, &out),
    }
}

fn validate_create(auth: &Auth, output: &FundingData) -> ScriptResult<()> {
    if auth.funding_authorized() && output.status == STATUS_READY {
        return Ok(());
    }
    Err(ScriptError::Unauthorized)
}

fn validate_delete(auth: &Auth, input: &FundingData) -> ScriptResult<()> {
    if auth.executor && (input.status == STATUS_ACTIVE || input.status == STATUS_FAILED) {
        return Ok(());
    }
    Err(ScriptError::BadTransition)
}

fn validate_update(auth: &Auth, input: &FundingData, output: &FundingData) -> ScriptResult<()> {
    if !auth.executor || input.amount != output.amount || output.status < input.status {
        return Err(ScriptError::BadTransition);
    }
    match output.status {
        STATUS_SUBMITTED => {
            if auth.funding_authorized() {
                Ok(())
            } else {
                Err(ScriptError::Unauthorized)
            }
        }
        STATUS_ACTIVE | STATUS_FAILED => Ok(()),
        _ => Err(ScriptError::BadTransition),
    }
}
