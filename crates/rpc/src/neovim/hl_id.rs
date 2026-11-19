use serde::{Deserialize, Serialize};

/// A highlight's id (`:help ui-event-hl_attr_define`): the attributes Neovim defined under it, which cells are drawn
/// with. Neovim sends it as a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Deserialize, Serialize)]
#[serde(transparent)]
pub struct HlId(pub u32);

impl HlId {
	/// The default highlight: Neovim's default colors, without attributes
	pub const DEFAULT: Self = Self(0);
}
