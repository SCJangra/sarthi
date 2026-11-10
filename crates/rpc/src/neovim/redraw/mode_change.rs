use serde::Deserialize;

/// The editor mode changed; the cursor should look like `mode_info[mode_idx]` from `mode_info_set`
/// (`:help ui-event-mode_change`)
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct ModeChangeEvent {
	pub mode: String,
	pub mode_idx: u32,
}

#[cfg(test)]
mod tests {
	use super::ModeChangeEvent;
	use crate::RedrawNotification;

	#[test]
	fn de_mode_change() {
		let expected =
			RedrawNotification::ModeChange(vec![ModeChangeEvent { mode: String::from("insert"), mode_idx: 3 }]);

		// ["mode_change", ["insert", 3]]
		let event = vec![
			0x92, 0xAB, b'm', b'o', b'd', b'e', b'_', b'c', b'h', b'a', b'n', b'g', b'e', 0x92, 0xA6, b'i', b'n', b's',
			b'e', b'r', b't', 0x03,
		];

		let deserialized: RedrawNotification = rmp_serde::from_slice(&event).unwrap();

		assert_eq!(deserialized, expected)
	}
}
