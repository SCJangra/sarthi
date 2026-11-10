use sarthi_derive::DeserializeTaggedEnum;

use crate::{FloatsChangedArgs, RedrawNotification, WinScrolledArgs};

#[derive(Debug, DeserializeTaggedEnum)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Notification {
	Redraw(Vec<RedrawNotification>),

	/// From sarthi's Lua (`crate::sarthi`)
	WinScrolled(WinScrolledArgs),

	/// From sarthi's Lua (`crate::sarthi`)
	FloatsChanged(FloatsChangedArgs),

	Other {
		method: String,
		value: rmpv::Value,
	},
}
