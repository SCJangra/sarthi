use serde::Deserialize;

use crate::ModeInfo;

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct ModeInfoSetEvent {
	pub cursor_style_enabled: bool,
	pub mode_info: Vec<ModeInfo>,
}

#[cfg(test)]
mod tests {
	use rmpv::Value;

	use super::ModeInfoSetEvent;
	use crate::RedrawNotification;
	use crate::{CursorShape, ModeInfo};

	#[test]
	fn de_mode_info_set() {
		let mode = |entries: &[(&str, Value)]| {
			Value::Map(entries.iter().map(|(key, value)| (Value::from(*key), value.clone())).collect())
		};
		// Like Neovim sends it: some keys missing in some modes, and keys sarthi doesn't use
		let normal = mode(&[
			("cursor_shape", "block".into()),
			("cell_percentage", 0.into()),
			("attr_id", 7.into()),
			("blinkon", 0.into()),
			("name", "normal".into()),
			("short_name", "n".into()),
		]);
		let insert =
			mode(&[("cursor_shape", "vertical".into()), ("cell_percentage", 25.into()), ("name", "insert".into())]);
		let event = Value::Array(vec![
			"mode_info_set".into(),
			Value::Array(vec![true.into(), Value::Array(vec![normal, insert])]),
		]);

		let expected = RedrawNotification::ModeInfoSet(vec![ModeInfoSetEvent {
			cursor_style_enabled: true,
			mode_info: vec![
				ModeInfo { cursor_shape: Some(CursorShape::Block), cell_percentage: Some(0), attr_id: Some(7) },
				ModeInfo { cursor_shape: Some(CursorShape::Vertical), cell_percentage: Some(25), attr_id: None },
			],
		}]);

		let bytes = rmp_serde::to_vec(&event).unwrap();
		assert_eq!(rmp_serde::from_slice::<RedrawNotification>(&bytes).unwrap(), expected);
	}
}
