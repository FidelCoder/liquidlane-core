#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use ckb_std::{
    ckb_constants::Source,
    high_level::{
        load_cell_capacity, load_cell_data, load_cell_lock_hash, load_cell_type_hash, load_script,
        load_script_hash, QueryIter,
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
}

pub type ScriptResult<T> = core::result::Result<T, ScriptError>;

impl From<ckb_std::error::SysError> for ScriptError {
    fn from(_: ckb_std::error::SysError) -> Self {
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

pub fn require_version(data: &[u8]) -> ScriptResult<()> {
    match data.first() {
        Some(version) if *version == VERSION => Ok(()),
        _ => Err(ScriptError::InvalidData),
    }
}

pub fn has_input_lock_hash(expected: &Hash) -> bool {
    QueryIter::new(load_cell_lock_hash, Source::Input).any(|hash| hash == *expected)
}

pub fn has_input_type_hash(expected: &Hash) -> bool {
    QueryIter::new(load_cell_type_hash, Source::Input).any(|hash| hash.as_ref() == Some(expected))
}

pub fn has_output_type_hash(expected: &Hash) -> bool {
    QueryIter::new(load_cell_type_hash, Source::Output).any(|hash| hash.as_ref() == Some(expected))
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

pub fn matching_data(type_hash: &Hash, source: Source) -> Vec<Vec<u8>> {
    QueryIter::new(load_cell_type_hash, source)
        .enumerate()
        .filter_map(|(index, hash)| {
            if hash.as_ref() == Some(type_hash) {
                load_cell_data(index, source).ok()
            } else {
                None
            }
        })
        .collect()
}

pub fn matching_capacity(type_hash: &Hash, source: Source) -> u64 {
    QueryIter::new(load_cell_type_hash, source)
        .enumerate()
        .filter_map(|(index, hash)| {
            if hash.as_ref() == Some(type_hash) {
                load_cell_capacity(index, source).ok()
            } else {
                None
            }
        })
        .sum()
}
