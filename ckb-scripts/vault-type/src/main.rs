#![no_std]
#![no_main]

use ckb_std::ckb_constants::Source;
use liquidlane_scripts_shared::{
    checked_sum, has_input_lock_hash, has_input_or_output_type_code_hash, read_hash, read_u64,
    require_exact_data, script_args, single_group_data, sum_capacity, sum_type_code_u64_field,
    Hash, ScriptError, ScriptResult, HASH_SIZE,
};

ckb_std::entry!(program_entry);
ckb_std::default_alloc!();
liquidlane_scripts_shared::ckb_panic_handler!();

const ADMIN_OFFSET: usize = 0;
const LP_RECEIPT_OFFSET: usize = HASH_SIZE;
const REQUEST_OFFSET: usize = HASH_SIZE * 2;
const FEE_CLAIM_OFFSET: usize = HASH_SIZE * 3;
const ARGS_LEN: usize = HASH_SIZE * 4;
const DATA_LEN: usize = 1 + 4 * 8;
const RECEIPT_DATA_LEN: usize = 1 + 5 * 8;
const REQUEST_DATA_LEN: usize = 1 + 1 + 3 * 8;
const CLAIM_DATA_LEN: usize = 1 + 1 + 8;
const RECEIPT_SUPPLIED_OFFSET: usize = 1;
const REQUEST_AMOUNT_OFFSET: usize = 2;
const REQUEST_FEE_OFFSET: usize = 10;
const CLAIM_AMOUNT_OFFSET: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
struct VaultData {
    total: u64,
    reserved: u64,
    deployed: u64,
    fee_balance: u64,
}

struct Args {
    admin_lock: Hash,
    lp_receipt_type: Hash,
    request_type: Hash,
    fee_claim_type: Hash,
}

struct PathAuth {
    admin: bool,
    receipt: bool,
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
    let input = single_group_data(Source::GroupInput)?.map(|data| VaultData::parse(&data));
    let output = single_group_data(Source::GroupOutput)?.map(|data| VaultData::parse(&data));
    validate_transition(&args, input.transpose()?, output.transpose()?)
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

impl VaultData {
    fn parse(data: &[u8]) -> ScriptResult<Self> {
        require_exact_data(data, DATA_LEN)?;
        let vault = Self {
            total: read_u64(data, 1)?,
            reserved: read_u64(data, 9)?,
            deployed: read_u64(data, 17)?,
            fee_balance: read_u64(data, 25)?,
        };
        if checked_sum(&[vault.reserved, vault.deployed])? > vault.total {
            return Err(ScriptError::InvalidData);
        }
        Ok(vault)
    }

    fn is_empty(&self) -> bool {
        self.total == 0 && self.reserved == 0 && self.deployed == 0 && self.fee_balance == 0
    }
}

impl PathAuth {
    fn load(args: &Args) -> Self {
        Self {
            admin: has_input_lock_hash(&args.admin_lock),
            receipt: has_input_or_output_type_code_hash(&args.lp_receipt_type),
            request: has_input_or_output_type_code_hash(&args.request_type),
            claim: has_input_or_output_type_code_hash(&args.fee_claim_type),
        }
    }

    fn any_service(&self) -> bool {
        self.receipt || self.request || self.claim
    }
}

fn validate_transition(
    args: &Args,
    input: Option<VaultData>,
    output: Option<VaultData>,
) -> ScriptResult<()> {
    let auth = PathAuth::load(args);
    match (input, output) {
        (None, None) => Err(ScriptError::BadTransition),
        (None, Some(out)) => validate_create(&auth, &out),
        (Some(inp), None) => validate_close(&auth, &inp),
        (Some(inp), Some(out)) => validate_update(args, &auth, &inp, &out),
    }
}

fn validate_create(auth: &PathAuth, output: &VaultData) -> ScriptResult<()> {
    if !auth.admin || !output.is_empty() {
        return Err(ScriptError::Unauthorized);
    }
    Ok(())
}

fn validate_close(auth: &PathAuth, input: &VaultData) -> ScriptResult<()> {
    if auth.admin && input.is_empty() {
        return Ok(());
    }
    Err(ScriptError::BadTransition)
}

fn validate_update(
    args: &Args,
    auth: &PathAuth,
    input: &VaultData,
    output: &VaultData,
) -> ScriptResult<()> {
    if !auth.admin && !auth.any_service() {
        return Err(ScriptError::Unauthorized);
    }
    require_capacity_delta(auth)?;
    require_accounting_delta(args, auth, input, output)
}

fn require_capacity_delta(auth: &PathAuth) -> ScriptResult<()> {
    let input_capacity = sum_capacity(Source::GroupInput);
    let output_capacity = sum_capacity(Source::GroupOutput);
    if output_capacity >= input_capacity || auth.receipt || auth.claim {
        return Ok(());
    }
    Err(ScriptError::ValueMismatch)
}

fn require_accounting_delta(
    args: &Args,
    auth: &PathAuth,
    input: &VaultData,
    output: &VaultData,
) -> ScriptResult<()> {
    if input.total != output.total {
        require_receipt_total_delta(args, auth, input.total, output.total)?;
    }
    if input.reserved != output.reserved || input.deployed != output.deployed {
        require_request_lock_delta(args, auth, input, output)?;
    }
    if input.fee_balance != output.fee_balance {
        require_fee_delta(args, auth, input.fee_balance, output.fee_balance)?;
    }
    Ok(())
}

fn require_receipt_total_delta(
    args: &Args,
    auth: &PathAuth,
    input_total: u64,
    output_total: u64,
) -> ScriptResult<()> {
    if !auth.receipt {
        return Err(ScriptError::BadTransition);
    }
    let receipt_input = sum_type_code_u64_field(
        &args.lp_receipt_type,
        Source::Input,
        RECEIPT_SUPPLIED_OFFSET,
        RECEIPT_DATA_LEN,
    )?;
    let receipt_output = sum_type_code_u64_field(
        &args.lp_receipt_type,
        Source::Output,
        RECEIPT_SUPPLIED_OFFSET,
        RECEIPT_DATA_LEN,
    )?;
    require_same_delta(input_total, output_total, receipt_input, receipt_output)
}

fn require_request_lock_delta(
    args: &Args,
    auth: &PathAuth,
    input: &VaultData,
    output: &VaultData,
) -> ScriptResult<()> {
    if !auth.request {
        return Err(ScriptError::BadTransition);
    }
    let input_locked = checked_sum(&[input.reserved, input.deployed])?;
    let output_locked = checked_sum(&[output.reserved, output.deployed])?;
    let delta = abs_delta(input_locked, output_locked);
    let request_limit = max_field_sum(&args.request_type, REQUEST_AMOUNT_OFFSET, REQUEST_DATA_LEN)?;
    if delta <= request_limit {
        return Ok(());
    }
    Err(ScriptError::ValueMismatch)
}

fn require_fee_delta(
    args: &Args,
    auth: &PathAuth,
    input_fee: u64,
    output_fee: u64,
) -> ScriptResult<()> {
    if output_fee > input_fee {
        let delta = output_fee - input_fee;
        let request_fee = max_field_sum(&args.request_type, REQUEST_FEE_OFFSET, REQUEST_DATA_LEN)?;
        if auth.request && delta <= request_fee {
            return Ok(());
        }
        return Err(ScriptError::ValueMismatch);
    }
    let delta = input_fee - output_fee;
    let claim_limit = max_field_sum(&args.fee_claim_type, CLAIM_AMOUNT_OFFSET, CLAIM_DATA_LEN)?;
    if auth.claim && delta <= claim_limit {
        return Ok(());
    }
    Err(ScriptError::ValueMismatch)
}

fn max_field_sum(code_hash: &Hash, offset: usize, data_len: usize) -> ScriptResult<u64> {
    let input = sum_type_code_u64_field(code_hash, Source::Input, offset, data_len)?;
    let output = sum_type_code_u64_field(code_hash, Source::Output, offset, data_len)?;
    Ok(core::cmp::max(input, output))
}

fn require_same_delta(
    input_value: u64,
    output_value: u64,
    service_input: u64,
    service_output: u64,
) -> ScriptResult<()> {
    match (
        output_value.cmp(&input_value),
        service_output.cmp(&service_input),
    ) {
        (core::cmp::Ordering::Equal, core::cmp::Ordering::Equal) => Ok(()),
        (core::cmp::Ordering::Greater, core::cmp::Ordering::Greater)
            if output_value - input_value == service_output - service_input =>
        {
            Ok(())
        }
        (core::cmp::Ordering::Less, core::cmp::Ordering::Less)
            if input_value - output_value == service_input - service_output =>
        {
            Ok(())
        }
        _ => Err(ScriptError::ValueMismatch),
    }
}

fn abs_delta(left: u64, right: u64) -> u64 {
    left.abs_diff(right)
}
