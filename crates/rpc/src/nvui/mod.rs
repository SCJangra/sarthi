//! nvui's own API, between nvui and its Lua: the requests the Lua sends, and the Lua itself

mod clipboard;
mod requests;
mod settings;

pub use clipboard::*;
pub use requests::*;
pub use settings::*;

/// nvui's Lua, run before the user's config, each file with nvui's channel as its argument: what it sets up, and its
/// code
pub const LUA: [(&str, &str); 2] = [
	("the clipboard", include_str!("../../lua/clipboard.lua")),
	("require(\"nvui\")", include_str!("../../lua/nvui.lua")),
];
