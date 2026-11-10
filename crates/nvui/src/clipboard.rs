use rpc::nvui::{Copied, Paste, Register};
use winit::window::Window;

/// The system clipboard, for Neovim's `+` and `*` registers
pub struct Clipboard {
	clipboard: window_clipboard::Clipboard,

	/// What nvui last copied to `+` and to `*`, with its register type, so pasting it back keeps the type
	copied: [Option<Copied>; 2],
}

impl Clipboard {
	/// Connects to the clipboard of `window`'s display
	pub fn connect(window: &Window) -> Result<Self, String> {
		// SAFETY: the display stays valid while `Clipboard` lives: the window state drops it before the window.
		let clipboard = unsafe { window_clipboard::Clipboard::connect(window) }.map_err(|err| err.to_string())?;
		Ok(Self { clipboard, copied: [None, None] })
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
		self.copied[register as usize] = Some(copied);
		Ok(())
	}

	pub fn paste(&self, register: Register) -> Result<Paste, String> {
		let read = match register {
			Register::Primary => self.clipboard.read_primary(),
			Register::Clipboard => None,
		};
		let text = read.unwrap_or_else(|| self.clipboard.read()).map_err(|err| err.to_string())?;
		Ok(paste(&text, self.copied[register as usize].as_ref()))
	}
}

/// What pasting `text` returns, given what nvui last `copied` to the register
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
	use rpc::nvui::{Copied, Paste};

	use super::paste;

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
	fn text_nvui_copied_keeps_its_type() {
		// Neovim copies a linewise yank with a trailing empty line.
		let copied = Copied { lines: lines(&["one", ""]), regtype: String::from("V") };
		assert_eq!(paste("one\n", Some(&copied)), Paste::Typed((lines(&["one", ""]), String::from("V"))));

		// Once something else changed the clipboard, the type no longer applies.
		assert_eq!(paste("other", Some(&copied)), Paste::Lines(lines(&["other"])));
	}
}
