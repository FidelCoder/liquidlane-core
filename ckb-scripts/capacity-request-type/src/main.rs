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
const MERCHANT_LOCK_OFFSET: usize = HASH_SIZE;
const OPERATOR_LOCK_OFFSET: usize = HASH_SIZE * 2;
const REQUEST_ID_OFFSET: usize = HASH_SIZE * 3;
const ARGS_LEN: usize = HASH_SIZE * 4;
const DATA_LEN: usize = 1 + 1 + 3 * 8;
const STATUS_PENDING: u8 = 0;
const STATUS_RESERVED: u8 = 1;
const STATUS_DEPLOYED: u8 = 2;
const STATUS_CLOSED: u8 = 3;

#[derive(Clone, Copy)]
struct RequestData {
    status: u8,
    amount: u64,
    lease_fee: u64,
    expiry: u64,
}

struct Args {
    vault_type: Hash,
    merchant_lock: Hash,
    operator_lock: Hash,
    _request_id: Hash,
}

struct Auth {
    merchant: bool,
    operator: bool,
    vault: bool,
}

pub fn program_entry() -> i8 {
    match main() {
        Ok(()) => 0,
        Err(error) => error as i8,
    }
}

fn main() -> ScriptResult<()> {
    let args = Args::load()?;
    let input = single_group_data(Source::GroupInput)?.map(|data| RequestData::parse(&data));
    let output = single_group_data(Source::GroupOutput)?.map(|data| RequestData::parse(&data));
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
            merchant_lock: read_hash(&bytes, MERCHANT_LOCK_OFFSET)?,
            operator_lock: read_hash(&bytes, OPERATOR_LOCK_OFFSET)?,
            _request_id: read_hash(&bytes, REQUEST_ID_OFFSET)?,
        })
    }
}

impl RequestData {
    fn parse(data: &[u8]) -> ScriptResult<Self> {
        require_exact_data(data, DATA_LEN)?;
        let request = Self {
            status: data[1],
            amount: read_u64(data, 2)?,
            lease_fee: read_u64(data, 10)?,
            expiry: read_u64(data, 18)?,
        };
        if request.status > STATUS_CLOSED
            || request.amount == 0
            || request.lease_fee == 0
            || request.expiry == 0
        {
            return Err(ScriptError::InvalidData);
        }
        Ok(request)
    }
}

impl Auth {
    fn load(args: &Args) -> Self {
        Self {
            merchant: has_input_lock_hash(&args.merchant_lock),
            operator: has_input_lock_hash(&args.operator_lock),
            vault: has_input_or_output_type_hash(&args.vault_type),
        }
    }
}

fn validate_transition(
    args: &Args,
    input: Option<RequestData>,
    output: Option<RequestData>,
) -> ScriptResult<()> {
    let auth = Auth::load(args);
    match (input, output) {
        (None, None) => Err(ScriptError::BadTransition),
        (None, Some(out)) => validate_create(&auth, &out),
        (Some(inp), None) => validate_delete(&auth, &inp),
        (Some(inp), Some(out)) => validate_update(&auth, &inp, &out),
    }
}

fn validate_create(auth: &Auth, output: &RequestData) -> ScriptResult<()> {
    let allowed_status = output.status == STATUS_PENDING || output.status == STATUS_RESERVED;
    if auth.merchant && auth.vault && allowed_status {
        return Ok(());
    }
    Err(ScriptError::Unauthorized)
}

fn validate_delete(auth: &Auth, input: &RequestData) -> ScriptResult<()> {
    if auth.merchant && input.status == STATUS_PENDING {
        return Ok(());
    }
    if auth.vault && input.status == STATUS_CLOSED {
        return Ok(());
    }
    Err(ScriptError::BadTransition)
}

fn validate_update(auth: &Auth, input: &RequestData, output: &RequestData) -> ScriptResult<()> {
    if !auth.merchant && !auth.operator {
        return Err(ScriptError::Unauthorized);
    }
    if input.amount != output.amount
        || input.lease_fee != output.lease_fee
        || input.expiry != output.expiry
    {
        return Err(ScriptError::BadTransition);
    }
    if output.status < input.status {
        return Err(ScriptError::BadTransition);
    }
    match output.status {
        STATUS_PENDING => Ok(()),
        STATUS_RESERVED => require_vault(auth),
        STATUS_DEPLOYED => require_operator_vault(auth),
        STATUS_CLOSED => require_vault(auth),
        _ => Err(ScriptError::InvalidData),
    }
}

fn require_vault(auth: &Auth) -> ScriptResult<()> {
    if auth.vault {
        Ok(())
    } else {
        Err(ScriptError::MissingVault)
    }
}

fn require_operator_vault(auth: &Auth) -> ScriptResult<()> {
    if auth.operator && auth.vault {
        Ok(())
    } else {
        Err(ScriptError::Unauthorized)
    }
}
