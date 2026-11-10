//! nvui's own API, between nvui and its Lua: the requests and notifications the Lua sends, and the Lua itself

mod clipboard;
mod notifications;
mod requests;
mod settings;

pub use clipboard::*;
pub use notifications::*;
pub use requests::*;
pub use settings::*;

/// What nvui runs before the user's config (`runtime/startup.lua`), with its channel and [`MODULES`] as the arguments
pub const STARTUP: &str = include_str!("../../../../runtime/startup.lua");

/// nvui's Lua modules (`runtime/lua/`), by module name, preloaded at startup
pub const MODULES: [(&str, &str); 4] = [
	("nvui", include_str!("../../../../runtime/lua/nvui/init.lua")),
	("nvui.rpc", include_str!("../../../../runtime/lua/nvui/rpc.lua")),
	("nvui.clipboard", include_str!("../../../../runtime/lua/nvui/clipboard.lua")),
	("nvui.scroll", include_str!("../../../../runtime/lua/nvui/scroll.lua")),
];

#[cfg(test)]
mod tests {
	use std::path::Path;

	use super::MODULES;

	#[test]
	fn every_module_is_embedded() {
		let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/lua");
		let mut files: Vec<String> = std::fs::read_dir(dir.join("nvui"))
			.unwrap()
			.map(|entry| entry.unwrap().file_name().into_string().unwrap())
			.collect();
		files.sort();

		let mut embedded: Vec<String> = MODULES
			.iter()
			.map(|(name, _)| match name.strip_prefix("nvui.") {
				Some(module) => format!("{module}.lua"),
				None => String::from("init.lua"),
			})
			.collect();
		embedded.sort();
		assert_eq!(files, embedded, "runtime/lua/nvui/ and MODULES differ");
	}
}
