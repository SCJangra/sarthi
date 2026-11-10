use nvui_derive::DeserializeTaggedEnum;

use crate::{RedrawNotification, WinScrolledArgs};

#[derive(Debug, DeserializeTaggedEnum)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Notification {
	Redraw(Vec<RedrawNotification>),

	/// From nvui's Lua (`crate::nvui`)
	WinScrolled(WinScrolledArgs),

	Other {
		method: String,
		value: rmpv::Value,
	},
}
