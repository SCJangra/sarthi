//! sarthi's development tasks. `cargo xtask docs` writes what's generated from the docs and types of
//! `crates/rpc/src/sarthi`: the help file, `runtime/doc/sarthi.txt`, rendered by pandoc's vimdoc writer, and the Lua
//! module's functions and types. `cargo xtask docs --check` only checks they're up to date.

mod help;
mod lua;
mod schema;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Width of the help file's text
const WIDTH: usize = 78;

fn main() {
	let args: Vec<String> = std::env::args().skip(1).collect();
	match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
		["docs"] => docs(false),
		["docs", "--check"] => docs(true),
		_ => {
			eprintln!("usage: cargo xtask docs [--check]");
			std::process::exit(2);
		},
	}
}

/// Writes the generated files, or with `check`, fails if one of them isn't what it would be
fn docs(check: bool) {
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
	let files: [(PathBuf, String); 4] = [
		(root.join("runtime/doc/sarthi.txt"), render(&root, &help::markdown(&root))),
		(root.join("runtime/lua/sarthi/init.lua"), lua::init()),
		(root.join("runtime/lua/sarthi/api.lua"), lua::api()),
		(root.join("runtime/lua/sarthi/_meta.lua"), lua::meta()),
	];

	let mut stale = Vec::new();
	for (path, content) in files {
		if check {
			if std::fs::read_to_string(&path).ok().as_deref() != Some(&content) {
				stale.push(path);
			}
		} else {
			std::fs::write(&path, content).unwrap_or_else(|error| panic!("writing {}: {error}", path.display()));
		}
	}
	if !stale.is_empty() {
		for path in stale {
			eprintln!("out of date: {}", path.display());
		}
		eprintln!("run `cargo xtask docs`");
		std::process::exit(1);
	}
}

/// `markdown` as a help file, by pandoc's vimdoc writer (pandoc 3.8.1 or later, in the dev shell). A warning from
/// pandoc, e.g. about a link it can't make, fails it.
fn render(root: &Path, markdown: &str) -> String {
	let mut pandoc = Command::new("pandoc")
		.args(["--from=markdown", "--to=vimdoc", "--standalone", "--fail-if-warnings"])
		// The sections, and in them the settings' sections and the window functions' tables
		.args(["--toc", "--toc-depth=2"])
		.arg(format!("--columns={WIDTH}"))
		.args(["--metadata=filename:sarthi.txt", "--metadata=title:Sarthi, a GUI for Neovim"])
		.arg(format!("--lua-filter={}", root.join("crates/xtask/links.lua").display()))
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.spawn()
		.expect("running pandoc (in the dev shell)");
	pandoc
		.stdin
		.take()
		.expect("pandoc's input")
		.write_all(markdown.as_bytes())
		.expect("writing to pandoc");
	let output = pandoc.wait_with_output().expect("waiting for pandoc");
	assert!(output.status.success(), "pandoc failed");
	String::from_utf8(output.stdout).expect("pandoc writes UTF-8")
}
