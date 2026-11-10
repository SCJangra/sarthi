use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct GridClearEvent {
	pub grid: u32,
}

#[cfg(test)]
mod tests {
	use super::GridClearEvent;
	use crate::RedrawNotification;

	#[test]
	fn de_grid_clear() {
		let expected = RedrawNotification::GridClear(vec![GridClearEvent { grid: 1 }, GridClearEvent { grid: 2 }]);

		let event =
			vec![0x93, 0xAA, 0x67, 0x72, 0x69, 0x64, 0x5F, 0x63, 0x6C, 0x65, 0x61, 0x72, 0x91, 0x01, 0x91, 0x02];

		let deserialized: RedrawNotification = rmp_serde::from_slice(&event).unwrap();

		assert_eq!(deserialized, expected)
	}
}
