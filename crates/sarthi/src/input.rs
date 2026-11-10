use winit::keyboard::{Key, KeyLocation, ModifiersState, NamedKey};

/// Translates a key press into Neovim's key notation (`:help key-notation`) for `nvim_input`, or `None` for keys
/// without one, e.g. a modifier on its own, or a dead key, whose character arrives with the next key press.
///
/// `key` and `text` are winit's `logical_key` and `text`, which ignore Ctrl: Ctrl+a arrives as "a" with Ctrl held.
pub fn neovim_keys(key: &Key, text: Option<&str>, location: KeyLocation, modifiers: ModifiersState) -> Option<String> {
	if let Some(name) = special_key(key, text, location) {
		return Some(notation(name, true, modifiers));
	}

	let text = text.or(match key {
		Key::Character(character) => Some(character.as_str()),
		_ => None,
	})?;

	// Keys Neovim has names for are handled above; any other control character has no notation.
	if text.chars().any(char::is_control) {
		return None;
	}
	Some(notation(text, false, modifiers))
}

/// Writes `key` with its modifiers. Shift is only written for special keys and for Ctrl with a letter: otherwise the
/// shifted character says it already (`A`, `$`), and Neovim reads e.g. `<M-S-a>` as `<M-A>`.
fn notation(key: &str, special: bool, modifiers: ModifiersState) -> String {
	let letter = key.len() == 1 && key.as_bytes()[0].is_ascii_alphabetic();
	// Neovim always uses uppercase for shifted letters, which winit doesn't always report.
	let key = if modifiers.shift_key() && letter { key.to_ascii_uppercase() } else { key.to_owned() };

	let mut prefix = String::new();
	if modifiers.shift_key() && (special || (modifiers.control_key() && letter)) {
		prefix.push_str("S-");
	}
	if modifiers.control_key() {
		prefix.push_str("C-");
	}
	if modifiers.alt_key() {
		prefix.push_str("M-");
	}
	if modifiers.super_key() {
		prefix.push_str("D-");
	}

	match (prefix.is_empty() && !special, key.as_str()) {
		// A literal "<" would start a key name
		(true, _) => key.replace('<', "<lt>"),
		(false, "<") => format!("<{prefix}lt>"),
		(false, _) => format!("<{prefix}{key}>"),
	}
}

/// Neovim's name for keys that aren't typed as text
fn special_key(key: &Key, text: Option<&str>, location: KeyLocation) -> Option<&'static str> {
	if location == KeyLocation::Numpad
		&& let Some(name) = keypad_key(key)
	{
		return Some(name);
	}

	let Key::Named(key) = key else { return None };
	let name = match key {
		NamedKey::Enter => "CR",
		NamedKey::Escape => "Esc",
		NamedKey::Backspace => "BS",
		NamedKey::Delete => "Del",
		NamedKey::Tab => "Tab",
		// Space can finish a dead key, producing its character instead.
		NamedKey::Space if matches!(text, None | Some(" ")) => "Space",
		NamedKey::ArrowUp => "Up",
		NamedKey::ArrowDown => "Down",
		NamedKey::ArrowLeft => "Left",
		NamedKey::ArrowRight => "Right",
		NamedKey::Home => "Home",
		NamedKey::End => "End",
		NamedKey::PageUp => "PageUp",
		NamedKey::PageDown => "PageDown",
		NamedKey::Insert => "Insert",
		NamedKey::Help => "Help",
		NamedKey::Undo => "Undo",
		NamedKey::Find => "Find",
		NamedKey::Select => "Select",
		_ => return function_key(key),
	};
	Some(name)
}

/// Keypad keys: digits and operators with NumLock on, navigation keys with it off
fn keypad_key(key: &Key) -> Option<&'static str> {
	let name = match key {
		Key::Character(character) => match character.as_str() {
			"0" => "k0",
			"1" => "k1",
			"2" => "k2",
			"3" => "k3",
			"4" => "k4",
			"5" => "k5",
			"6" => "k6",
			"7" => "k7",
			"8" => "k8",
			"9" => "k9",
			"+" => "kPlus",
			"-" => "kMinus",
			"*" => "kMultiply",
			"/" => "kDivide",
			"." => "kPoint",
			"," => "kComma",
			"=" => "kEqual",
			_ => return None,
		},
		Key::Named(key) => match key {
			NamedKey::Enter => "kEnter",
			NamedKey::Home => "kHome",
			NamedKey::End => "kEnd",
			NamedKey::PageUp => "kPageUp",
			NamedKey::PageDown => "kPageDown",
			NamedKey::ArrowUp => "kUp",
			NamedKey::ArrowDown => "kDown",
			NamedKey::ArrowLeft => "kLeft",
			NamedKey::ArrowRight => "kRight",
			NamedKey::Insert => "kInsert",
			NamedKey::Delete => "kDel",
			// The middle key (5) with NumLock off
			NamedKey::Clear => "kOrigin",
			_ => return None,
		},
		_ => return None,
	};
	Some(name)
}

/// F1 to F35, all that winit reports
fn function_key(key: &NamedKey) -> Option<&'static str> {
	macro_rules! function_keys {
		($($name:ident)*) => {
			match key {
				$(NamedKey::$name => Some(stringify!($name)),)*
				_ => None,
			}
		};
	}

	function_keys!(
		F1 F2 F3 F4 F5 F6 F7 F8 F9 F10 F11 F12 F13 F14 F15 F16 F17 F18
		F19 F20 F21 F22 F23 F24 F25 F26 F27 F28 F29 F30 F31 F32 F33 F34 F35
	)
}

#[cfg(test)]
mod tests {
	use winit::keyboard::{Key, KeyLocation, ModifiersState, NamedKey};

	use super::neovim_keys;

	const NONE: ModifiersState = ModifiersState::empty();
	const SHIFT: ModifiersState = ModifiersState::SHIFT;
	const CTRL: ModifiersState = ModifiersState::CONTROL;
	const ALT: ModifiersState = ModifiersState::ALT;
	const SUPER: ModifiersState = ModifiersState::SUPER;

	/// A key that types `text`, like winit reports it
	fn text(text: &str, modifiers: ModifiersState) -> Option<String> {
		neovim_keys(&Key::Character(text.into()), Some(text), KeyLocation::Standard, modifiers)
	}

	fn named(key: NamedKey, text: Option<&str>, modifiers: ModifiersState) -> Option<String> {
		neovim_keys(&Key::Named(key), text, KeyLocation::Standard, modifiers)
	}

	fn keypad(key: Key, text: Option<&str>, modifiers: ModifiersState) -> Option<String> {
		neovim_keys(&key, text, KeyLocation::Numpad, modifiers)
	}

	#[test]
	fn plain_text() {
		assert_eq!(text("a", NONE).as_deref(), Some("a"));
		assert_eq!(text("é", NONE).as_deref(), Some("é"));
		assert_eq!(text("漢", NONE).as_deref(), Some("漢"));
	}

	#[test]
	fn shifted_text_is_just_the_character() {
		assert_eq!(text("A", SHIFT).as_deref(), Some("A"));
		assert_eq!(text("$", SHIFT).as_deref(), Some("$"));
		// winit may report a shifted letter in lowercase
		assert_eq!(text("a", SHIFT).as_deref(), Some("A"));
	}

	#[test]
	fn less_than_is_escaped() {
		assert_eq!(text("<", SHIFT).as_deref(), Some("<lt>"));
		assert_eq!(text("<", CTRL).as_deref(), Some("<C-lt>"));
	}

	#[test]
	fn modifiers_with_text() {
		assert_eq!(text("a", CTRL).as_deref(), Some("<C-a>"));
		assert_eq!(text("A", CTRL | SHIFT).as_deref(), Some("<S-C-A>"));
		assert_eq!(text("a", CTRL | SHIFT).as_deref(), Some("<S-C-A>"));
		assert_eq!(text("a", ALT).as_deref(), Some("<M-a>"));
		assert_eq!(text("A", ALT | SHIFT).as_deref(), Some("<M-A>"));
		assert_eq!(text("$", ALT | SHIFT).as_deref(), Some("<M-$>"));
		assert_eq!(text("1", CTRL).as_deref(), Some("<C-1>"));
		assert_eq!(text("s", SUPER).as_deref(), Some("<D-s>"));
		assert_eq!(text("x", CTRL | ALT | SUPER).as_deref(), Some("<C-M-D-x>"));
	}

	#[test]
	fn special_keys() {
		assert_eq!(named(NamedKey::Enter, Some("\r"), NONE).as_deref(), Some("<CR>"));
		assert_eq!(named(NamedKey::Escape, Some("\u{1b}"), NONE).as_deref(), Some("<Esc>"));
		assert_eq!(named(NamedKey::Backspace, Some("\u{8}"), NONE).as_deref(), Some("<BS>"));
		assert_eq!(named(NamedKey::Delete, Some("\u{7f}"), NONE).as_deref(), Some("<Del>"));
		assert_eq!(named(NamedKey::Tab, Some("\t"), NONE).as_deref(), Some("<Tab>"));
		assert_eq!(named(NamedKey::ArrowLeft, None, NONE).as_deref(), Some("<Left>"));
		assert_eq!(named(NamedKey::PageDown, None, NONE).as_deref(), Some("<PageDown>"));
		assert_eq!(named(NamedKey::F1, None, NONE).as_deref(), Some("<F1>"));
		assert_eq!(named(NamedKey::F35, None, NONE).as_deref(), Some("<F35>"));
	}

	#[test]
	fn special_keys_always_include_shift() {
		assert_eq!(named(NamedKey::Tab, Some("\t"), SHIFT).as_deref(), Some("<S-Tab>"));
		assert_eq!(named(NamedKey::F1, None, SHIFT).as_deref(), Some("<S-F1>"));
		assert_eq!(named(NamedKey::ArrowUp, None, CTRL | SHIFT).as_deref(), Some("<S-C-Up>"));
		assert_eq!(named(NamedKey::Enter, Some("\r"), ALT).as_deref(), Some("<M-CR>"));
	}

	#[test]
	fn space() {
		assert_eq!(named(NamedKey::Space, Some(" "), NONE).as_deref(), Some("<Space>"));
		assert_eq!(named(NamedKey::Space, Some(" "), CTRL).as_deref(), Some("<C-Space>"));
		assert_eq!(named(NamedKey::Space, Some(" "), SHIFT).as_deref(), Some("<S-Space>"));
		// Space after a dead key types the dead key's character.
		assert_eq!(named(NamedKey::Space, Some("´"), NONE).as_deref(), Some("´"));
	}

	#[test]
	fn keypad_with_numlock() {
		assert_eq!(keypad(Key::Character("7".into()), Some("7"), NONE).as_deref(), Some("<k7>"));
		assert_eq!(keypad(Key::Character("+".into()), Some("+"), NONE).as_deref(), Some("<kPlus>"));
		assert_eq!(keypad(Key::Character(".".into()), Some("."), NONE).as_deref(), Some("<kPoint>"));
		assert_eq!(keypad(Key::Named(NamedKey::Enter), Some("\r"), NONE).as_deref(), Some("<kEnter>"));
		assert_eq!(keypad(Key::Character("1".into()), Some("1"), CTRL).as_deref(), Some("<C-k1>"));
	}

	#[test]
	fn keypad_without_numlock() {
		assert_eq!(keypad(Key::Named(NamedKey::Home), None, NONE).as_deref(), Some("<kHome>"));
		assert_eq!(keypad(Key::Named(NamedKey::ArrowUp), None, NONE).as_deref(), Some("<kUp>"));
		assert_eq!(keypad(Key::Named(NamedKey::Clear), None, NONE).as_deref(), Some("<kOrigin>"));
		assert_eq!(keypad(Key::Named(NamedKey::Delete), None, NONE).as_deref(), Some("<kDel>"));
	}

	#[test]
	fn keys_without_notation() {
		assert_eq!(named(NamedKey::Shift, None, SHIFT), None);
		assert_eq!(named(NamedKey::Control, None, CTRL), None);
		assert_eq!(neovim_keys(&Key::Dead(Some('´')), None, KeyLocation::Standard, NONE), None);
		assert_eq!(named(NamedKey::CapsLock, None, NONE), None);
		assert_eq!(text("\u{1}", NONE), None);
	}
}
