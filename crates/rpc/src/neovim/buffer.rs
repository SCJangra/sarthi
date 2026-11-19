use serde::{Deserialize, Serialize};

/// A buffer's handle (`:help bufnr()`), as Lua passes it: a number
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Buffer(pub i64);
