use nvui_derive::DeserializeTaggedEnum;

use crate::RedrawNotification;

#[derive(Debug, DeserializeTaggedEnum)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Notification {
	Redraw(Vec<RedrawNotification>),

	Other { method: String, value: rmpv::Value },
}
