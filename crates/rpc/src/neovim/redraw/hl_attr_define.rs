use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct HlAttrDefineEvent {
	/// Highlight id.
	pub id: u32,

	/// RGB attributes.
	pub rgb_attr: HlAttrs,

	/// Cterm attributes.
	pub cterm_attr: HlAttrs,

	/// Semantic highlight info (ext_hlstate).
	pub info: Vec<HlAttrInfo>,
}

#[derive(Debug, Deserialize, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub struct HlAttrs {
	/// Foreground color.
	#[serde(default)]
	pub foreground: Option<u32>,

	/// Background color.
	#[serde(default)]
	pub background: Option<u32>,

	/// Special color (underline/undercurl).
	#[serde(default)]
	pub special: Option<u32>,

	/// Reverse video.
	#[serde(default)]
	pub reverse: bool,

	/// Italic text.
	#[serde(default)]
	pub italic: bool,

	/// Bold text.
	#[serde(default)]
	pub bold: bool,

	/// Strikethrough text.
	#[serde(default)]
	pub strikethrough: bool,

	/// Underline text.
	#[serde(default)]
	pub underline: bool,

	/// Undercurl text.
	#[serde(default)]
	pub undercurl: bool,

	/// Double underline text.
	#[serde(default)]
	pub underdouble: bool,

	/// Dotted underline text.
	#[serde(default)]
	pub underdotted: bool,

	/// Dashed underline text.
	#[serde(default)]
	pub underdashed: bool,

	/// Alternative font.
	#[serde(default)]
	pub altfont: bool,

	/// Dim text.
	#[serde(default)]
	pub dim: bool,

	/// Blinking text.
	#[serde(default)]
	pub blink: bool,

	/// Concealed text.
	#[serde(default)]
	pub conceal: bool,

	/// Overline text.
	#[serde(default)]
	pub overline: bool,

	/// Blend level (0-100).
	#[serde(default)]
	pub blend: Option<u8>,

	/// URL associated with the highlight.
	#[serde(default)]
	pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct HlAttrInfo {
	/// Semantic highlight kind.
	pub kind: HlAttrKind,

	/// Builtin highlight group name.
	#[serde(default)]
	pub ui_name: Option<String>,

	/// Final highlight group name.
	#[serde(default)]
	pub hi_name: Option<String>,

	/// Highlight item id.
	#[serde(default)]
	pub id: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(rename_all = "lowercase")]
pub enum HlAttrKind {
	Ui,
	Syntax,
	Terminal,
	#[serde(other)]
	Unknown,
}

#[cfg(test)]
mod tests {
	use super::{HlAttrDefineEvent, HlAttrInfo, HlAttrKind, HlAttrs};
	use crate::RedrawNotification;

	#[test]
	fn de_hl_attr_define() {
		let expected = RedrawNotification::HlAttrDefine(vec![HlAttrDefineEvent {
			id: 1,
			rgb_attr: HlAttrs {
				foreground: Some(10),
				background: None,
				special: None,
				reverse: false,
				italic: false,
				bold: true,
				strikethrough: false,
				underline: false,
				undercurl: false,
				underdouble: false,
				underdotted: false,
				underdashed: false,
				altfont: false,
				dim: false,
				blink: false,
				conceal: false,
				overline: false,
				blend: None,
				url: None,
			},
			cterm_attr: HlAttrs {
				foreground: Some(20),
				background: None,
				special: None,
				reverse: false,
				italic: false,
				bold: false,
				strikethrough: false,
				underline: false,
				undercurl: false,
				underdouble: false,
				underdotted: false,
				underdashed: false,
				altfont: false,
				dim: false,
				blink: false,
				conceal: false,
				overline: false,
				blend: None,
				url: None,
			},
			info: vec![HlAttrInfo {
				kind: HlAttrKind::Ui,
				ui_name: Some(String::from("Normal")),
				hi_name: Some(String::from("Normal")),
				id: Some(1),
			}],
		}]);

		let event = vec![
			0x92, 0xAE, 0x68, 0x6C, 0x5F, 0x61, 0x74, 0x74, 0x72, 0x5F, 0x64, 0x65, 0x66, 0x69, 0x6E, 0x65, 0x94, 0x01,
			0x82, 0xAA, 0x66, 0x6F, 0x72, 0x65, 0x67, 0x72, 0x6F, 0x75, 0x6E, 0x64, 0x0A, 0xA4, 0x62, 0x6F, 0x6C, 0x64,
			0xC3, 0x81, 0xAA, 0x66, 0x6F, 0x72, 0x65, 0x67, 0x72, 0x6F, 0x75, 0x6E, 0x64, 0x14, 0x91, 0x84, 0xA4, 0x6B,
			0x69, 0x6E, 0x64, 0xA2, 0x75, 0x69, 0xA7, 0x75, 0x69, 0x5F, 0x6E, 0x61, 0x6D, 0x65, 0xA6, 0x4E, 0x6F, 0x72,
			0x6D, 0x61, 0x6C, 0xA7, 0x68, 0x69, 0x5F, 0x6E, 0x61, 0x6D, 0x65, 0xA6, 0x4E, 0x6F, 0x72, 0x6D, 0x61, 0x6C,
			0xA2, 0x69, 0x64, 0x01,
		];

		let deserialized: RedrawNotification = rmp_serde::from_slice(&event).unwrap();

		assert_eq!(deserialized, expected)
	}
}
