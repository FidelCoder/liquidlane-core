use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CkbScript {
    pub code_hash: String,
    pub hash_type: String,
    pub args: String,
}
