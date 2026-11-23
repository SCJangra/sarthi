//! The shadows under floating and external windows, shared by windows that belong together so they read as one.

use std::collections::HashMap;

use renderer::{LayerShadow, Rect};
use rpc::neovim::{Grid, Window};

use crate::external::{Place, Relative};
use crate::{Editor, Placement};

impl Editor {
	/// The shadows under the floating windows among `placed`, by the grid of the window each is drawn with. Floats at
	/// one `zindex` with the same shadow share it, so the windows of a plugin's UI read as one; a float attached to
	/// another counts at that float's level. A level's first shadow is drawn with its first window, under all of them;
	/// another, for windows with a shadow of their own, with the first window casting it.
	pub(crate) fn float_shadows(&self, placed: &[(Grid, &Placement)]) -> HashMap<Grid, LayerShadow> {
		let floats: Vec<(Grid, Window, u32, [u32; 2])> = placed
			.iter()
			.filter_map(|&(id, placement)| match placement {
				Placement::Float { win, row, col, zindex, .. } => Some((id, *win, *zindex, [*col, *row])),
				_ => None,
			})
			.collect();
		let zindex: HashMap<Window, u32> = floats.iter().map(|&(_, win, zindex, _)| (win, zindex)).collect();
		let parent = |win: &Window| {
			self.floats
				.get(win)
				.and_then(|float| float.parent)
				.filter(|parent| zindex.contains_key(parent))
		};
		// Up to the float it's attached to, if any, and on from there; at most once per float, so a loop ends.
		let level = |mut win: Window| {
			for _ in 0..floats.len() {
				let Some(next) = parent(&win) else { break };
				win = next;
			}
			zindex[&win]
		};

		// By level and shadow, in the order of their first windows: the level, the grid it's drawn with, and the shadow.
		let mut groups: Vec<(u32, Grid, LayerShadow)> = Vec::new();
		let mut first_of_level: HashMap<u32, Grid> = HashMap::new();
		for &(id, win, _, [col, row]) in &floats {
			let level = level(win);
			let first = *first_of_level.entry(level).or_insert(id);
			let Some(shadow) = self.shadow_of(win) else { continue };
			let (grid, cell) = (&self.grids[&id], self.cell);
			let [width, height] = [grid.width * cell.width, grid.height * cell.height];
			// Its cells': a float has no frame.
			let rect = Rect::new(col * cell.width, row * cell.height, width, height);
			let same = groups
				.iter_mut()
				.find(|(group_level, _, group)| *group_level == level && group.shadow == shadow);
			match same {
				Some((_, _, group)) => group.area.push(rect),
				None => {
					let carrier = if groups.iter().any(|(group_level, ..)| *group_level == level) { id } else { first };
					// Square, like its cells.
					groups.push((level, carrier, LayerShadow { shadow, area: vec![rect], radius: 0.0 }));
				},
			}
		}
		groups.into_iter().map(|(_, carrier, group)| (carrier, group)).collect()
	}

	/// The shadows under the external windows among `placed`, by the grid of the window each is drawn with. Like
	/// attached floats, external windows placed relative to another external window, and so on, share its shadow if
	/// it's the same, so windows put together read as one; it's drawn with the first of them, under all of them.
	pub(crate) fn external_shadows(&self, placed: &[(Grid, &Placement)]) -> HashMap<Grid, LayerShadow> {
		let grid_of = |win: Window| {
			let (&grid, _) = self.viewports.iter().find(|(_, viewport)| viewport.win == Some(win))?;
			matches!(self.placements.get(&grid), Some(Placement::External { .. })).then_some(grid)
		};
		// The external window it's placed relative to, if any, and on from there; a few at most, so a loop ends.
		let root = |mut win: Window| {
			for _ in 0..8 {
				let Some(Place::Relative { relative: Relative::Win(next), .. }) = self.places.get(&win) else { break };
				if grid_of(*next).is_none() {
					break;
				}
				win = *next;
			}
			win
		};

		// By root and shadow, in the order of their first windows: the root, the grid it's drawn with, and the shadow.
		let mut groups: Vec<(Window, Grid, LayerShadow)> = Vec::new();
		for &(id, placement) in placed {
			let Placement::External { win, .. } = placement else { continue };
			let Some(shadow) = self.shadow_of(*win) else { continue };
			let rect = self.shadow_area(*win, self.external_rect(id, *win));
			let (root, radius) = (root(*win), self.shadow_radius(*win));
			match groups
				.iter_mut()
				.find(|(group_root, _, group)| *group_root == root && group.shadow == shadow)
			{
				Some((_, _, group)) => {
					group.area.push(rect);
					group.radius = group.radius.max(radius);
				},
				None => groups.push((root, id, LayerShadow { shadow, area: vec![rect], radius })),
			}
		}
		groups.into_iter().map(|(_, carrier, group)| (carrier, group)).collect()
	}
}
