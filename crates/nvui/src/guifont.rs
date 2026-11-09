//! The `guifont` option (`:help 'guifont'`), read the way GUIs for Neovim read it: font families separated by commas,
//! with options after colons, e.g. `JetBrains Mono,Symbols Nerd Font:h14:b`.

/// What `guifont` asks for
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuiFont {
	/// The main font, then fallbacks for the characters it lacks. Empty for nvui's default font.
	pub families: Vec<String>,

	/// In points (`:h14`); `None` for nvui's default size
	pub size: Option<f32>,

	/// All text bold (`:b`) or italic (`:i`)
	pub bold: bool,
	pub italic: bool,
}

/// Logical pixels in a point: a point is 1/72 inch, a logical pixel 1/96.
const PIXELS_PER_POINT: f32 = 96.0 / 72.0;

impl GuiFont {
	/// Reads `guifont`. Options may follow any family; underscores in names stand for spaces, like in gVim.
	pub fn parse(value: &str) -> Result<Self, String> {
		let mut font = Self::default();
		for entry in value.split(',') {
			let mut parts = entry.split(':');
			let family = parts.next().unwrap_or_default().trim().replace('_', " ");
			if !family.is_empty() {
				font.families.push(family);
			}

			for option in parts.map(str::trim) {
				match option.split_at_checked(1) {
					Some(("h", size)) => {
						let size = size.parse().ok().filter(|size: &f32| size.is_finite() && *size > 0.0);
						font.size = Some(size.ok_or_else(|| format!("invalid size: {option}"))?);
					},
					Some(("b", "")) => font.bold = true,
					Some(("i", "")) => font.italic = true,
					_ if option.is_empty() => (),
					_ => return Err(format!("unknown option: {option} (nvui knows h<size>, b and i)")),
				}
			}
		}
		Ok(font)
	}

	/// The size in logical pixels, if `guifont` has one
	pub fn pixels(&self) -> Option<f32> {
		self.size.map(|points| points * PIXELS_PER_POINT)
	}
}

#[cfg(test)]
mod tests {
	use super::GuiFont;

	fn families(names: &[&str]) -> Vec<String> {
		names.iter().map(|name| name.to_string()).collect()
	}

	#[test]
	fn empty_means_defaults() {
		assert_eq!(GuiFont::parse("").unwrap(), GuiFont::default());
	}

	#[test]
	fn families_and_options() {
		let font = GuiFont::parse("JetBrains Mono,Symbols Nerd Font:h13.5:b").unwrap();
		let expected = GuiFont {
			families: families(&["JetBrains Mono", "Symbols Nerd Font"]),
			size: Some(13.5),
			bold: true,
			italic: false,
		};
		assert_eq!(font, expected);

		// Options after any family, underscores for spaces, spaces around commas
		let font = GuiFont::parse("Fira_Code:h12, Noto Color Emoji:i").unwrap();
		assert_eq!(font.families, families(&["Fira Code", "Noto Color Emoji"]));
		assert_eq!((font.size, font.italic), (Some(12.0), true));

		// Only a size
		assert_eq!(GuiFont::parse(":h11").unwrap(), GuiFont { size: Some(11.0), ..GuiFont::default() });
	}

	#[test]
	fn sizes_are_points() {
		assert_eq!(GuiFont::parse("X:h12").unwrap().pixels(), Some(16.0));
		assert_eq!(GuiFont::parse("X").unwrap().pixels(), None);
	}

	#[test]
	fn errors() {
		assert_eq!(GuiFont::parse("X:h0").unwrap_err(), "invalid size: h0");
		assert_eq!(GuiFont::parse("X:hbig").unwrap_err(), "invalid size: hbig");
		assert!(GuiFont::parse("X:w500").unwrap_err().starts_with("unknown option: w500"));
	}
}
