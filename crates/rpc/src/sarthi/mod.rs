//! sarthi's own API, between sarthi and its Lua: the requests and notifications the Lua sends, and the Lua itself

mod clipboard;
mod notifications;
mod place;
mod requests;
mod settings;

pub use clipboard::*;
pub use notifications::*;
pub use place::*;
pub use requests::*;
pub use settings::*;

/// What sarthi runs before the user's config (`runtime/startup.lua`), with its channel and [`MODULES`] as the arguments
pub const STARTUP: &str = include_str!("../../../../runtime/startup.lua");

/// sarthi's Lua modules (`runtime/lua/`), by module name, preloaded at startup
pub const MODULES: [(&str, &str); 6] = [
	("sarthi", include_str!("../../../../runtime/lua/sarthi/init.lua")),
	("sarthi.api", include_str!("../../../../runtime/lua/sarthi/api.lua")),
	("sarthi.rpc", include_str!("../../../../runtime/lua/sarthi/rpc.lua")),
	("sarthi.clipboard", include_str!("../../../../runtime/lua/sarthi/clipboard.lua")),
	("sarthi.scroll", include_str!("../../../../runtime/lua/sarthi/scroll.lua")),
	("sarthi.floats", include_str!("../../../../runtime/lua/sarthi/floats.lua")),
];

#[cfg(test)]
mod tests {
	use std::path::Path;

	use super::MODULES;

	#[test]
	fn every_module_is_embedded() {
		let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/lua");
		let mut files: Vec<String> = std::fs::read_dir(dir.join("sarthi"))
			.unwrap()
			.map(|entry| entry.unwrap().file_name().into_string().unwrap())
			.collect();
		files.sort();

		let mut embedded: Vec<String> = MODULES
			.iter()
			.map(|(name, _)| match name.strip_prefix("sarthi.") {
				Some(module) => format!("{module}.lua"),
				None => String::from("init.lua"),
			})
			.collect();
		embedded.sort();
		assert_eq!(files, embedded, "runtime/lua/sarthi/ and MODULES differ");
	}
}
