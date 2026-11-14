//! sarthi's command line: its own few options, then Neovim's arguments, e.g. `sarthi -d a b` or `sarthi +42 file`

use std::ffi::OsString;

use clap::{ArgAction, Parser};

/// A GUI for Neovim
#[derive(Debug, Parser)]
#[command(
	version,
	// `-V` is Neovim's (verbose), so only `--version` is sarthi's.
	disable_version_flag = true,
	after_help = "Arguments after sarthi's options are Neovim's, e.g. `sarthi -d a b` or `sarthi +42 file`; after `--`, \
	              all are, even sarthi's options."
)]
pub struct Args {
	/// The Neovim to run
	#[arg(long, value_name = "PATH", default_value = "nvim")]
	pub nvim: OsString,

	/// Print sarthi's version
	#[arg(long, action = ArgAction::Version)]
	version: (),

	/// Neovim's arguments, e.g. files to edit
	#[arg(value_name = "NEOVIM ARGUMENTS", trailing_var_arg = true, allow_hyphen_values = true)]
	pub args: Vec<OsString>,
}

#[cfg(test)]
mod tests {
	use clap::Parser;
	use clap::error::ErrorKind;

	use super::Args;

	/// Neovim, and its arguments, for `args`
	fn parse(args: &[&str]) -> (String, Vec<String>) {
		let parsed = Args::try_parse_from(["sarthi"].iter().chain(args)).unwrap();
		let strings = |args: Vec<_>| {
			args.into_iter()
				.map(|arg: std::ffi::OsString| arg.into_string().unwrap())
				.collect()
		};
		(parsed.nvim.into_string().unwrap(), strings(parsed.args))
	}

	fn kind(args: &[&str]) -> ErrorKind {
		Args::try_parse_from(["sarthi"].iter().chain(args)).unwrap_err().kind()
	}

	#[test]
	fn the_rest_is_neovim_s() {
		assert_eq!(parse(&[]), ("nvim".into(), vec![]));
		assert_eq!(parse(&["-d", "a", "b"]), ("nvim".into(), vec!["-d".into(), "a".into(), "b".into()]));
		assert_eq!(parse(&["+42", "file"]), ("nvim".into(), vec!["+42".into(), "file".into()]));
		// Neovim's own `-V` (verbose), not sarthi's version
		assert_eq!(parse(&["-V9", "file"]), ("nvim".into(), vec!["-V9".into(), "file".into()]));
	}

	#[test]
	fn options() {
		assert_eq!(parse(&["--nvim", "/opt/nvim", "file"]), ("/opt/nvim".into(), vec!["file".into()]));
		assert_eq!(parse(&["--nvim=/opt/nvim"]).0, "/opt/nvim");
		assert_eq!(kind(&["--version"]), ErrorKind::DisplayVersion);
		assert_eq!(kind(&["--help"]), ErrorKind::DisplayHelp);
		assert_eq!(kind(&["--nvim"]), ErrorKind::InvalidValue);
		// After Neovim's arguments, or `--`, all Neovim's
		assert_eq!(parse(&["file", "--nvim", "x"]), ("nvim".into(), vec!["file".into(), "--nvim".into(), "x".into()]));
		assert_eq!(parse(&["--", "--help"]), ("nvim".into(), vec!["--help".into()]));
	}
}
