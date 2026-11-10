use renderer::CellSize;
use winit::dpi::PhysicalSize;

/// Dimensions of a grid, in cells
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridSize {
	pub width: u32,
	pub height: u32,
}

impl GridSize {
	/// Returns how many whole cells of `cell_size` fit in `window_size`, but at least 1×1.
	pub fn fit(window_size: PhysicalSize<u32>, cell_size: CellSize) -> Self {
		let cells = |pixels: u32, cell: u32| (pixels / cell).max(1);

		Self { width: cells(window_size.width, cell_size.width), height: cells(window_size.height, cell_size.height) }
	}
}

#[cfg(test)]
mod tests {
	use renderer::CellSize;
	use winit::dpi::PhysicalSize;

	use super::GridSize;

	#[test]
	fn fit_whole_cells() {
		let size = GridSize::fit(PhysicalSize::new(1280, 720), CellSize { width: 9, height: 18 });
		assert_eq!(size, GridSize { width: 142, height: 40 });
	}

	#[test]
	fn fit_hidpi() {
		let size = GridSize::fit(PhysicalSize::new(2560, 1440), CellSize { width: 18, height: 36 });
		assert_eq!(size, GridSize { width: 142, height: 40 });
	}

	#[test]
	fn fit_at_least_one_cell() {
		let size = GridSize::fit(PhysicalSize::new(0, 5), CellSize { width: 9, height: 18 });
		assert_eq!(size, GridSize { width: 1, height: 1 });
	}
}
