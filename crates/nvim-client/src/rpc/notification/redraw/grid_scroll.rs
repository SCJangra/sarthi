use serde::Deserialize;

/// Moves the cells in rows `top..bot` and columns `left..right` of `grid` up by `rows` (down if negative). The
/// rows that open up are filled by `grid_line` events that follow (`:help ui-event-grid_scroll`).
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct GridScrollEvent {
	pub grid: u32,
	pub top: u32,
	pub bot: u32,
	pub left: u32,
	pub right: u32,
	pub rows: i32,

	/// Always 0, reserved by Neovim for future use
	pub cols: i32,
}

#[cfg(test)]
mod tests {
	use super::GridScrollEvent;
	use crate::rpc::notification::redraw::RedrawNotification;

	#[test]
	fn de_grid_scroll() {
		let expected = RedrawNotification::GridScroll(vec![
			GridScrollEvent { grid: 1, top: 0, bot: 10, left: 0, right: 80, rows: 2, cols: 0 },
			GridScrollEvent { grid: 2, top: 1, bot: 5, left: 0, right: 40, rows: -3, cols: 0 },
		]);

		// ["grid_scroll", [1, 0, 10, 0, 80, 2, 0], [2, 1, 5, 0, 40, -3, 0]]
		let event = vec![
			0x93, 0xAB, 0x67, 0x72, 0x69, 0x64, 0x5F, 0x73, 0x63, 0x72, 0x6F, 0x6C, 0x6C, 0x97, 0x01, 0x00, 0x0A, 0x00,
			0x50, 0x02, 0x00, 0x97, 0x02, 0x01, 0x05, 0x00, 0x28, 0xFD, 0x00,
		];

		let deserialized: RedrawNotification = rmp_serde::from_slice(&event).unwrap();

		assert_eq!(deserialized, expected)
	}
}
