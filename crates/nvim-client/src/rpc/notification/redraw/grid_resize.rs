use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct GridResizeEvent {
	pub grid: u32,
	pub width: u32,
	pub height: u32,
}

#[cfg(test)]
mod tests {
	use super::GridResizeEvent;
	use crate::rpc::notification::redraw::RedrawNotification;

	#[test]
	fn de_grid_resize() {
		let expected = RedrawNotification::GridResize(vec![
			GridResizeEvent { grid: 1, width: 20, height: 20 },
			GridResizeEvent { grid: 2, width: 40, height: 40 },
		]);

		let event = vec![
			0x93, 0xAB, 0x67, 0x72, 0x69, 0x64, 0x5F, 0x72, 0x65, 0x73, 0x69, 0x7A, 0x65, 0x93, 0x01, 0x14, 0x14, 0x93,
			0x02, 0x28, 0x28,
		];

		let deserialized: RedrawNotification = rmp_serde::from_slice(&event).unwrap();

		assert_eq!(deserialized, expected)
	}
}
