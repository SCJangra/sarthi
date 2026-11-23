//! How an API function is named and typed, for calls and notifications.

use serde::Serialize;
use serde::de::DeserializeOwned;

/// An API function: its name, and the types of its arguments and result.
pub trait RpcMethod {
	/// Its name, e.g. `nvim_input`.
	const METHOD: &'static str;

	/// Its arguments, as Neovim takes them.
	type Params: Serialize;
	/// What it returns.
	type Response: DeserializeOwned;
}
