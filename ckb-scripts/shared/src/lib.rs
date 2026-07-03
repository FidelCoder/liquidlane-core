#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use ckb_std::{
    ckb_constants::Source,
    ckb_types::prelude::Entity,
    error::SysError,
    high_level::{
        load_cell_capacity, load_cell_data, load_cell_lock_hash, load_cell_type,
        load_cell_type_hash, load_script, load_script_hash, QueryIter,
    },
};

#[macro_export]
macro_rules! ckb_panic_handler {
    () => {
        #[panic_handler]
        fn panic(_info: &core::panic::PanicInfo) -> ! {
            loop {}
        }
    };
}

pub type Hash = [u8; 32];

pub const HASH_SIZE: usize = 32;
pub const U64_SIZE: usize = 8;
pub const VERSION: u8 = 1;

#[repr(i8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptError {
    Syscall = 1,
    InvalidArgs = 2,
    InvalidData = 3,
    Unauthorized = 4,
    MissingVault = 5,
    ValueMismatch = 6,
    BadTransition = 7,
    DuplicateCell = 8,
    Arithmetic = 9,
}

pub type ScriptResult<T> = core::result::Result<T, ScriptError>;

impl From<SysError> for ScriptError {
    fn from(_: SysError) -> Self {
        ScriptError::Syscall
    }
}

pub fn script_args() -> ScriptResult<Vec<u8>> {
    Ok(load_script()?.args().raw_data().to_vec())
}

pub fn current_script_hash() -> ScriptResult<Hash> {
    Ok(load_script_hash()?)
}

pub fn read_hash(bytes: &[u8], offset: usize) -> ScriptResult<Hash> {
    let end = offset
        .checked_add(HASH_SIZE)
        .ok_or(ScriptError::InvalidArgs)?;
    let slice = bytes.get(offset..end).ok_or(ScriptError::InvalidArgs)?;
    let mut hash = [0u8; HASH_SIZE];
    hash.copy_from_slice(slice);
    Ok(hash)
}

pub fn read_u64(bytes: &[u8], offset: usize) -> ScriptResult<u64> {
    let end = offset
        .checked_add(U64_SIZE)
        .ok_or(ScriptError::InvalidData)?;
    let slice = bytes.get(offset..end).ok_or(ScriptError::InvalidData)?;
    let mut raw = [0u8; U64_SIZE];
    raw.copy_from_slice(slice);
    Ok(u64::from_le_bytes(raw))
}

pub fn require_exact_data(data: &[u8], expected_len: usize) -> ScriptResult<()> {
    if data.len() != expected_len {
        return Err(ScriptError::InvalidData);
    }
    match data.first() {
        Some(version) if *version == VERSION => Ok(()),
        _ => Err(ScriptError::InvalidData),
    }
}

pub fn has_input_lock_hash(expected: &Hash) -> bool {
    QueryIter::new(load_cell_lock_hash, Source::Input).any(|hash| hash == *expected)
}

pub fn has_type_hash(expected: &Hash, source: Source) -> bool {
    QueryIter::new(load_cell_type_hash, source).any(|hash| hash.as_ref() == Some(expected))
}

pub fn has_input_type_hash(expected: &Hash) -> bool {
    has_type_hash(expected, Source::Input)
}

pub fn has_output_type_hash(expected: &Hash) -> bool {
    has_type_hash(expected, Source::Output)
}

pub fn has_input_or_output_type_hash(expected: &Hash) -> bool {
    has_input_type_hash(expected) || has_output_type_hash(expected)
}

pub fn has_type_code_hash(expected: &Hash, source: Source) -> bool {
    QueryIter::new(load_cell_type, source).any(|script| match script {
        Some(script) => script.code_hash().as_slice() == expected,
        None => false,
    })
}

pub fn has_input_or_output_type_code_hash(expected: &Hash) -> bool {
    has_type_code_hash(expected, Source::Input) || has_type_code_hash(expected, Source::Output)
}

pub fn count_type_hash(expected: &Hash, source: Source) -> usize {
    QueryIter::new(load_cell_type_hash, source)
        .filter(|hash| hash.as_ref() == Some(expected))
        .count()
}

pub fn count_lock_hash(expected: &Hash, source: Source) -> usize {
    QueryIter::new(load_cell_lock_hash, source)
        .filter(|hash| hash == expected)
        .count()
}

pub fn cell_count(source: Source) -> usize {
    QueryIter::new(load_cell_capacity, source).count()
}

pub fn single_group_data(source: Source) -> ScriptResult<Option<Vec<u8>>> {
    match cell_count(source) {
        0 => Ok(None),
        1 => Ok(Some(load_cell_data(0, source)?)),
        _ => Err(ScriptError::DuplicateCell),
    }
}

pub fn sum_capacity(source: Source) -> u64 {
    QueryIter::new(load_cell_capacity, source).sum()
}

pub fn sum_type_code_u64_field(
    expected_code_hash: &Hash,
    source: Source,
    offset: usize,
    expected_len: usize,
) -> ScriptResult<u64> {
    let mut total = 0u64;
    for (index, script) in QueryIter::new(load_cell_type, source).enumerate() {
        let matches = match script {
            Some(script) => script.code_hash().as_slice() == expected_code_hash,
            None => false,
        };
        if !matches {
            continue;
        }
        let data = load_cell_data(index, source)?;
        require_exact_data(&data, expected_len)?;
        total = total
            .checked_add(read_u64(&data, offset)?)
            .ok_or(ScriptError::Arithmetic)?;
    }
    Ok(total)
}

pub fn checked_sum(values: &[u64]) -> ScriptResult<u64> {
    values.iter().try_fold(0u64, |sum, value| {
        sum.checked_add(*value).ok_or(ScriptError::Arithmetic)
    })
}
