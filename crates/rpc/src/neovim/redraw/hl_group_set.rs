use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct HlGroupSetEvent {
	/// Highlight group name.
	pub name: String,

	/// Highlight attribute id.
	pub hl_id: u32,
}

#[cfg(test)]
mod tests {
	use super::HlGroupSetEvent;
	use crate::RedrawNotification;

	#[test]
	fn de_hl_group_set() {
		let expected = RedrawNotification::HlGroupSet(vec![HlGroupSetEvent { name: String::from("Normal"), hl_id: 3 }]);

		let event = vec![
			0x92, 0xAC, 0x68, 0x6C, 0x5F, 0x67, 0x72, 0x6F, 0x75, 0x70, 0x5F, 0x73, 0x65, 0x74, 0x92, 0xA6, 0x4E, 0x6F,
			0x72, 0x6D, 0x61, 0x6C, 0x03,
		];

		let deserialized: RedrawNotification = rmp_serde::from_slice(&event).unwrap();

		assert_eq!(deserialized, expected)
	}
}
