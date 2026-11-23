use serde::{Deserialize, Serialize};

/// A grid's handle (`:help ui-multigrid`): what Neovim draws a window, or the screen, to. Neovim sends it as a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Grid(pub u32);

impl Grid {
	/// The global grid, the whole screen: what isn't in a window, like window separators, status lines and the command
	/// line, with the windows' grids placed on it.
	pub const GLOBAL: Self = Self(1);
}

impl std::fmt::Display for Grid {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		self.0.fmt(formatter)
	}
}
