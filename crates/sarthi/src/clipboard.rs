use std::time::{Duration, Instant};

use rpc::sarthi::{Copied, Paste, Register};
use winit::window::Window;

/// The system clipboard, for Neovim's `+` and `*` registers
pub struct Clipboard {
	clipboard: window_clipboard::Clipboard,
	copies: Copies,
}

impl Clipboard {
	/// Connects to the clipboard of `window`'s display
	pub fn connect(window: &Window) -> Result<Self, String> {
		// SAFETY: the display stays valid while `Clipboard` lives: the window state drops it before the window.
		let clipboard = unsafe { window_clipboard::Clipboard::connect(window) }.map_err(|err| err.to_string())?;
		Ok(Self { clipboard, copies: Copies::default() })
	}

	pub fn copy(&mut self, register: Register, copied: Copied) -> Result<(), String> {
		let text = copied.lines.join("\n");
		let written = match register {
			// Platforms without a primary selection use the clipboard for both.
			Register::Primary => self.clipboard.write_primary(text.clone()),
			Register::Clipboard => None,
		};
		written
			.unwrap_or_else(|| self.clipboard.write(text))
			.map_err(|err| err.to_string())?;
		self.copies.copied(register, copied, Instant::now());
		Ok(())
	}

	pub fn paste(&mut self, register: Register) -> Result<Paste, String> {
		let read = match register {
			Register::Primary => self.clipboard.read_primary(),
			Register::Clipboard => None,
		};
		let text = read.unwrap_or_else(|| self.clipboard.read()).map_err(|err| err.to_string());
		self.copies.paste(register, text, Instant::now())
	}
}

/// How long the system clipboard may take to show sarthi's copy: on Wayland, the compositor announces it a moment after
/// sarthi copied, and reading before gets what it held before.
const ANNOUNCE_TIME: Duration = Duration::from_millis(500);

/// What sarthi copied to `+` and to `*`
#[derive(Debug, Default)]
struct Copies {
	/// The last copy to each register, with its register type, so pasting it back keeps the type
	last: [Option<Copied>; 2],

	/// When sarthi copied to each register, until the system clipboard shows the copy
	unannounced: [Option<Instant>; 2],
}

impl Copies {
	fn copied(&mut self, register: Register, copied: Copied, now: Instant) {
		self.last[register as usize] = Some(copied);
		self.unannounced[register as usize] = Some(now);
	}

	/// What pasting from `register` returns, given what the system clipboard held (`read`). Shortly after a copy that
	/// it doesn't show yet, it's the copy.
	fn paste(&mut self, register: Register, read: Result<String, String>, now: Instant) -> Result<Paste, String> {
		let index = register as usize;
		let pasted = read.map(|text| paste(&text, self.last[index].as_ref()));
		if let Ok(Paste::Typed(_)) = pasted {
			self.unannounced[index] = None;
			return pasted;
		}

		let unannounced = self.unannounced[index].is_some_and(|copied| now.duration_since(copied) < ANNOUNCE_TIME);
		match &self.last[index] {
			Some(copied) if unannounced => Ok(Paste::Typed((copied.lines.clone(), copied.regtype.clone()))),
			_ => pasted,
		}
	}
}

/// What pasting `text` returns, given what sarthi last `copied` to the register
fn paste(text: &str, copied: Option<&Copied>) -> Paste {
	let lines: Vec<String> = text
		.split('\n')
		.map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
		.collect();
	match copied {
		Some(copied) if copied.lines == lines => Paste::Typed((lines, copied.regtype.clone())),
		_ => Paste::Lines(lines),
	}
}

#[cfg(test)]
mod tests {
	use std::time::{Duration, Instant};

	use rpc::sarthi::{Copied, Paste, Register};

	use super::{ANNOUNCE_TIME, Copies, paste};

	fn lines(lines: &[&str]) -> Vec<String> {
		lines.iter().map(|line| line.to_string()).collect()
	}

	#[test]
	fn text_from_elsewhere_pastes_as_lines() {
		assert_eq!(paste("one\ntwo", None), Paste::Lines(lines(&["one", "two"])));
		// A trailing newline leaves an empty last line, which Neovim takes as linewise.
		assert_eq!(paste("one\r\ntwo\r\n", None), Paste::Lines(lines(&["one", "two", ""])));
	}

	#[test]
	fn text_sarthi_copied_keeps_its_type() {
		// Neovim copies a linewise yank with a trailing empty line.
		let copied = Copied { lines: lines(&["one", ""]), regtype: String::from("V") };
		assert_eq!(paste("one\n", Some(&copied)), Paste::Typed((lines(&["one", ""]), String::from("V"))));

		// Once something else changed the clipboard, the type no longer applies.
		assert_eq!(paste("other", Some(&copied)), Paste::Lines(lines(&["other"])));
	}

	#[test]
	fn a_copy_pastes_before_the_clipboard_shows_it() {
		let start = Instant::now();
		let later = |millis| start + Duration::from_millis(millis);
		let copy = || Paste::Typed((lines(&["one"]), String::from("v")));
		let mut copies = Copies::default();
		copies.copied(Register::Clipboard, Copied { lines: lines(&["one"]), regtype: String::from("v") }, start);

		// Read too early, the clipboard still holds what it held before, or nothing.
		assert_eq!(copies.paste(Register::Clipboard, Ok(String::from("before")), later(1)), Ok(copy()));
		assert_eq!(copies.paste(Register::Clipboard, Err(String::from("selection is empty")), later(2)), Ok(copy()));
		// The other register isn't affected.
		let primary = copies.paste(Register::Primary, Ok(String::from("other")), later(3));
		assert_eq!(primary, Ok(Paste::Lines(lines(&["other"]))));

		// Once the clipboard showed the copy, it's trusted: another app may copy right after.
		assert_eq!(copies.paste(Register::Clipboard, Ok(String::from("one")), later(4)), Ok(copy()));
		let pasted = copies.paste(Register::Clipboard, Ok(String::from("theirs")), later(5));
		assert_eq!(pasted, Ok(Paste::Lines(lines(&["theirs"]))));

		// Never shown, e.g. if the copy didn't reach the clipboard, the clipboard is trusted after a while.
		copies.copied(Register::Clipboard, Copied { lines: lines(&["two"]), regtype: String::from("v") }, later(10));
		let pasted = copies.paste(Register::Clipboard, Ok(String::from("theirs")), later(10) + ANNOUNCE_TIME);
		assert_eq!(pasted, Ok(Paste::Lines(lines(&["theirs"]))));
	}
}
