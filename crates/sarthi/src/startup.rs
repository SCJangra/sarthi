//! Starting Neovim before the window opens: with the login shell's environment when started from a desktop launcher,
//! and at least [`MINIMUM`]. When it can't start, the window opens anyway and shows why, in an external window sarthi
//! gives its content itself, as Neovim would: the same events, so it's drawn, placed and framed like Neovim's.

use std::ffi::{OsStr, OsString};
use std::io::{IsTerminal, Read};
use std::os::unix::ffi::OsStrExt;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;
use std::{fmt, io, thread};

use editor::Editor;
use flume::Receiver;
use rpc::client::{Error as RpcError, Message, Nvim};
use rpc::neovim::{
	CellText, DefaultColorsSetEvent, GridCell, GridLineEvent, GridResizeEvent, HlAttrDefineEvent, HlAttrs,
	RedrawNotification, WinExternalPosEvent, Window,
};
use rpc::sarthi::{FloatWindow, Settings, WindowOptions, WindowVariable};

use crate::settings;

/// The oldest Neovim sarthi works with, as major and minor version
pub const MINIMUM: (u64, u64) = (0, 12);

/// How long the login shell gets to print its environment
const SHELL_TIMEOUT: Duration = Duration::from_secs(2);

/// Why Neovim didn't start
#[derive(Debug, PartialEq)]
pub enum StartError {
	NotFound {
		nvim: OsString,
	},
	/// Older than [`MINIMUM`]; its version, if it could tell
	TooOld {
		nvim: OsString,
		version: Option<String>,
	},
	Failed {
		nvim: OsString,
		error: String,
	},
}

impl StartError {
	/// A heading, and what happened and what to do
	fn text(&self) -> (String, String) {
		let (major, minor) = MINIMUM;
		let hint = "or start Sarthi with --nvim /path/to/nvim.";
		match self {
			Self::NotFound { nvim } => (
				String::from("Sarthi couldn't find Neovim"),
				format!("`{}` wasn't found. Install Neovim {major}.{minor} or newer, {hint}", nvim.display()),
			),
			Self::TooOld { nvim, version } => (
				format!("Sarthi needs Neovim {major}.{minor} or newer"),
				format!(
					"`{}` is {}. Update Neovim, {hint}",
					nvim.display(),
					version.as_deref().unwrap_or("older than 0.9")
				),
			),
			Self::Failed { nvim, error } => {
				(String::from("Sarthi couldn't start Neovim"), format!("`{}`: {error}", nvim.display()))
			},
		}
	}
}

impl fmt::Display for StartError {
	fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		let (heading, body) = self.text();
		write!(formatter, "{heading}: {body}")
	}
}

/// Starts Neovim, `nvim`, with `args`, in `env` (or sarthi's), if it's at least [`MINIMUM`]
pub fn start(
	nvim: &OsStr,
	args: &[OsString],
	env: Option<&[(OsString, OsString)]>,
) -> Result<(Nvim, Receiver<Message>), StartError> {
	let (client, messages) = Nvim::start(nvim, args, env).map_err(|error| match error {
		RpcError::SpawnNvim(error) if error.kind() == io::ErrorKind::NotFound => {
			StartError::NotFound { nvim: nvim.to_owned() }
		},
		RpcError::SpawnNvim(error) => StartError::Failed { nvim: nvim.to_owned(), error: error.to_string() },
		error => StartError::Failed { nvim: nvim.to_owned(), error: error.to_string() },
	})?;
	// `vim.version()` is from 0.9: older ones fail here.
	let version =
		client.exec_lua::<(u64, u64, u64)>("local v = vim.version() return { v.major, v.minor, v.patch }", ());
	match version {
		Ok((major, minor, _)) if (major, minor) >= MINIMUM => Ok((client, messages)),
		Ok((major, minor, patch)) => {
			Err(StartError::TooOld { nvim: nvim.to_owned(), version: Some(format!("{major}.{minor}.{patch}")) })
		},
		Err(_) => Err(StartError::TooOld { nvim: nvim.to_owned(), version: None }),
	}
}

/// The login shell's environment, when sarthi wasn't started from a terminal: a desktop launcher's often lacks what the
/// user's profile adds to `PATH`, where Neovim and the tools it runs are. `None` from a terminal, or if the shell fails
/// or takes too long.
pub fn login_environment() -> Option<Vec<(OsString, OsString)>> {
	if io::stdin().is_terminal() {
		return None;
	}
	let shell = std::env::var_os("SHELL")?;
	let mut child = Command::new(&shell)
		.args(["-l", "-c", "env -0"])
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::null())
		.spawn()
		.ok()?;
	let mut stdout = child.stdout.take()?;
	let (sender, receiver) = mpsc::channel();
	thread::spawn(move || {
		let mut output = Vec::new();
		let _ = stdout.read_to_end(&mut output);
		let _ = sender.send(output);
	});
	let output = receiver.recv_timeout(SHELL_TIMEOUT);
	if output.is_err() {
		tracing::warn!("{} took too long to give its environment; Neovim gets sarthi's", shell.display());
		let _ = child.kill();
	}
	let _ = child.wait();
	let env = environment(&output.ok()?);
	env.iter().any(|(name, _)| name == "PATH").then_some(env)
}

/// The variables in `env -0`'s output. What a profile printed before them, like a greeting, ends with a newline, which
/// no variable's name has: it's left out.
fn environment(output: &[u8]) -> Vec<(OsString, OsString)> {
	output
		.split(|byte| *byte == 0)
		.filter_map(|entry| {
			let at = entry.iter().position(|byte| *byte == b'=')?;
			let (name, value) = (&entry[..at], &entry[at + 1..]);
			let name = name.rsplit(|byte| *byte == b'\n').next().unwrap_or(name);
			let valid = !name.is_empty() && !name.iter().any(|byte| byte.is_ascii_whitespace());
			valid.then(|| (OsStr::from_bytes(name).to_owned(), OsStr::from_bytes(value).to_owned()))
		})
		.collect()
}

// Neovim's default dark colors (`:help default-colors`), what it looks like before a config loads
const NORMAL_FG: u32 = 0xE0E2EA;
const NORMAL_BG: u32 = 0x14161B;
const FLOAT_BG: u32 = 0x07080D;
const ERROR_FG: u32 = 0xFFC0B9;
const COMMENT_FG: u32 = 0x9B9EA4;

/// The grid and window of the error, which no Neovim uses
const GRID: u32 = 2;
const WIN: Window = Window(1000);

/// Shows `error` on a `width` × `height` cell screen: centered, in an external window framed like Neovim's floats
pub fn show(editor: &mut Editor, error: &StartError, width: u32, height: u32) {
	let (heading, body) = error.text();
	let text_width = width.saturating_sub(8).clamp(20, 64) as usize;
	let mut lines = vec![(heading, 1), (String::new(), 2)];
	lines.extend(wrap(&body, text_width).into_iter().map(|line| (line, 2)));
	lines.extend([(String::new(), 2), (String::from("Press Enter, Esc or q to close"), 3)]);
	let grid_width = lines.iter().map(|(line, _)| line.chars().count()).max().unwrap_or(0) as u32;

	let attrs = |foreground, bold| HlAttrs {
		foreground: Some(foreground),
		background: Some(FLOAT_BG),
		bold,
		..HlAttrs::default()
	};
	let define = |id, rgb_attr| HlAttrDefineEvent { id, rgb_attr, cterm_attr: HlAttrs::default(), info: vec![] };
	let rows = lines.iter().enumerate().map(|(row, (line, hl_id))| {
		let mut cells: Vec<GridCell> = line
			.chars()
			.map(|c| GridCell { text: cell(&c.to_string()), hl_id: Some(*hl_id), repeat: None })
			.collect();
		// The rest of the row in its background too
		let rest = grid_width.saturating_sub(cells.len() as u32);
		if rest > 0 {
			cells.push(GridCell { text: cell(" "), hl_id: Some(*hl_id), repeat: Some(rest) });
		}
		GridLineEvent { grid: GRID, row: row as u32, col_start: 0, cells, wrap: false }
	});
	let mut events = vec![
		RedrawNotification::DefaultColorsSet(vec![DefaultColorsSetEvent {
			rgb_fg: NORMAL_FG,
			rgb_bg: NORMAL_BG,
			rgb_sp: ERROR_FG,
			cterm_fg: 0,
			cterm_bg: 0,
		}]),
		RedrawNotification::HlAttrDefine(vec![
			define(1, attrs(ERROR_FG, true)),
			define(2, attrs(NORMAL_FG, false)),
			define(3, attrs(COMMENT_FG, false)),
		]),
		RedrawNotification::GridResize(vec![
			GridResizeEvent { grid: 1, width, height },
			GridResizeEvent { grid: GRID, width: grid_width, height: lines.len() as u32 },
		]),
	];
	events.extend(rows.map(|row| RedrawNotification::GridLine(vec![row])));
	events.extend([
		RedrawNotification::WinExternalPos(vec![WinExternalPosEvent { grid: GRID, win: WIN }]),
		// No cursor, as Neovim hides it while busy
		RedrawNotification::BusyStart,
		RedrawNotification::Flush,
	]);
	editor.apply(events);

	// Like any window opened with `require('sarthi').api`, with the default settings: nothing of its own
	let window = FloatWindow {
		win: WIN,
		filetype: String::new(),
		sarthi: WindowVariable::default(),
		winblend: 0,
		outline_color: None,
		background: Some(FLOAT_BG),
		parent: None,
	};
	let managed = WindowOptions { managed: Some(true), ..WindowOptions::default() };
	editor.set_float_style(WIN, settings::float_style(&Settings::default(), &window, &managed));
}

fn cell(text: &str) -> CellText {
	CellText::from(text).unwrap_or_default()
}

/// `text` in lines of at most `width` characters, broken between words
fn wrap(text: &str, width: usize) -> Vec<String> {
	let mut lines = Vec::new();
	let mut line = String::new();
	for word in text.split_whitespace() {
		if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
			lines.push(std::mem::take(&mut line));
		}
		if !line.is_empty() {
			line.push(' ');
		}
		line.push_str(word);
	}
	if !line.is_empty() {
		lines.push(line);
	}
	lines
}

#[cfg(test)]
mod tests {
	use std::ffi::OsString;

	use editor::Editor;

	use super::{StartError, environment, show, start, wrap};

	#[test]
	fn not_found() {
		let missing = OsString::from("/nonexistent/nvim");
		let error = start(&missing, &[], None).err().unwrap();
		assert_eq!(error, StartError::NotFound { nvim: missing });
		assert!(
			error
				.to_string()
				.starts_with("Sarthi couldn't find Neovim: `/nonexistent/nvim` wasn't found.")
		);
	}

	#[test]
	fn the_neovim_here_is_new_enough() {
		let (nvim, _messages) =
			start(&OsString::from("nvim"), &["--clean", "-i", "NONE", "-n"].map(OsString::from), None).unwrap();
		drop(nvim);
	}

	#[test]
	fn too_old() {
		let error = StartError::TooOld { nvim: "nvim".into(), version: Some(String::from("0.10.2")) };
		assert_eq!(
			error.to_string(),
			"Sarthi needs Neovim 0.12 or newer: `nvim` is 0.10.2. Update Neovim, or start Sarthi with --nvim /path/to/nvim."
		);
	}

	#[test]
	fn environment_from_env_0() {
		let output = b"Welcome!\nPATH=/a:/b\0HOME=/home/me\0EMPTY=\0LINES=one\ntwo\0";
		let env = environment(output);
		let env: Vec<(&str, &str)> = env
			.iter()
			.map(|(name, value)| (name.to_str().unwrap(), value.to_str().unwrap()))
			.collect();
		// The greeting before the first variable isn't part of it.
		assert_eq!(env, [("PATH", "/a:/b"), ("HOME", "/home/me"), ("EMPTY", ""), ("LINES", "one\ntwo")]);
	}

	#[test]
	fn wrapping() {
		assert_eq!(wrap("one two three four", 9), ["one two", "three", "four"]);
		assert_eq!(wrap("", 9), Vec::<String>::new());
	}

	#[test]
	fn shown_in_an_external_window() {
		let mut editor = Editor::new();
		let error = StartError::NotFound { nvim: "nvim".into() };
		show(&mut editor, &error, 80, 24);
		let redraw = editor.refresh().expect("drawn");
		// The screen, and the error's window over it
		assert_eq!(redraw.layers.len(), 2);
		let screen = &redraw.layers[0];
		assert!(screen.cursor.is_none());
		assert_eq!(redraw.background, 0x14161B);
	}
}
