//! The notifications Neovim sends sarthi: redraws, and those of sarthi's Lua

use sarthi_derive::DeserializeTaggedEnum;

use crate::{CmdheightChangedArgs, FloatsChangedArgs, RedrawNotification, WinScrolledArgs};

/// A notification from Neovim
#[derive(Debug, DeserializeTaggedEnum)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Notification {
	/// What Neovim redrew (`:help ui-events`)
	Redraw(Vec<RedrawNotification>),

	/// From sarthi's Lua (`crate::sarthi`)
	WinScrolled(WinScrolledArgs),

	/// From sarthi's Lua (`crate::sarthi`)
	FloatsChanged(FloatsChangedArgs),

	/// From sarthi's Lua (`crate::sarthi`)
	CmdheightChanged(CmdheightChangedArgs),

	/// A notification sarthi doesn't handle
	Other {
		/// Its name
		method: String,
		/// Its arguments, undecoded
		value: rmpv::Value,
	},
}
