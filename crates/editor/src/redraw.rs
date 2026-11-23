//! Neovim's redraw events (`:help ui-events`) applied to the editor's state, drawn once a redraw ends.

use renderer::{Edges, Layer};
use rpc::neovim::{CellText, Grid, OptionSetEvent, OptionValue, RedrawNotification, WinViewportEvent, Window};

use crate::grid::GridState;
use crate::preedit::PREEDIT_ID;
use crate::{Editor, Placement, Redraw};

impl Editor {
	/// Applies a batch of redraw events in order. Neovim's state is only consistent once a redraw ends (`flush`), so
	/// that's when this returns what to draw, if anything changed.
	pub fn apply(&mut self, events: Vec<RedrawNotification>) -> Option<Redraw> {
		let mut redraw = None;
		for event in events {
			redraw = self.apply_event(event).or(redraw);
		}
		redraw
	}
	fn apply_event(&mut self, event: RedrawNotification) -> Option<Redraw> {
		match event {
			// A grid starts with its first resize.
			RedrawNotification::GridResize(resizes) => {
				for resize in resizes {
					let grid = self.grids.entry(resize.grid).or_insert_with(|| GridState::new(0, 0));
					grid.resize(resize.width, resize.height);
					self.stale = true;
				}
			},
			RedrawNotification::GridDestroy(destroys) => {
				for destroy in destroys {
					self.destroy_grid(destroy.grid);
				}
			},
			RedrawNotification::GridClear(clears) => {
				for clear in clears {
					self.update_grid(clear.grid, "grid_clear", GridState::clear);
				}
			},
			RedrawNotification::GridLine(lines) => {
				for line in lines {
					self.update_grid(line.grid, "grid_line", |grid| {
						grid.set_line(line.row, line.col_start, &line.cells);
					});
				}
			},
			RedrawNotification::GridScroll(scrolls) => {
				for scroll in scrolls {
					self.update_grid(scroll.grid, "grid_scroll", |grid| {
						grid.scroll(scroll.top, scroll.bot, scroll.left, scroll.right, scroll.rows);
					});
				}
			},
			RedrawNotification::GridCursorGoto(gotos) => {
				for goto in gotos {
					self.update_grid(goto.grid, "grid_cursor_goto", |grid| {
						grid.set_cursor(goto.row, goto.col);
					});
					self.cursor_grid = goto.grid;
				}
			},
			RedrawNotification::WinPos(positions) => {
				for position in positions {
					self.place(position.grid, Placement::Tiled { row: position.start_row, col: position.start_col });
				}
			},
			RedrawNotification::WinFloatPos(positions) => {
				for position in positions {
					let (row, col, compindex) = (position.screen_row, position.screen_col, position.compindex);
					let (win, zindex, mouse) = (position.win, position.zindex, position.mouse_enabled);
					self.place(position.grid, Placement::Float { win, row, col, zindex, compindex, mouse });
				}
			},
			RedrawNotification::WinExternalPos(positions) => {
				for position in positions {
					self.place_external(position.grid, position.win);
				}
			},
			RedrawNotification::WinHide(hides) => {
				for hide in hides {
					self.place(hide.grid, Placement::Hidden);
				}
			},
			RedrawNotification::WinClose(closes) => {
				for close in closes {
					self.place(close.grid, Placement::Hidden);
				}
			},
			RedrawNotification::MsgSetPos(positions) => {
				for position in positions {
					let separator = CellText::from(&position.sep_char)
						.ok()
						.filter(|_| position.scrolled && position.row > 0);
					self.place(
						position.grid,
						Placement::Message { row: position.row, compindex: position.compindex, separator },
					);
				}
			},
			RedrawNotification::WinViewport(viewports) => {
				for event in viewports {
					self.set_viewport(event);
				}
			},
			RedrawNotification::WinViewportMargins(margins) => {
				for event in margins {
					self.viewports.entry(event.grid).or_default().margins =
						Edges { top: event.top, right: event.right, bottom: event.bottom, left: event.left };
				}
			},
			RedrawNotification::HlAttrDefine(attrs) => {
				self.highlights.set_attrs(attrs);
				self.stale = true;
			},
			RedrawNotification::DefaultColorsSet(colors) => {
				self.highlights.set_default_colors(colors);
				self.stale = true;
			},
			RedrawNotification::HlGroupSet(groups) => self.highlights.set_hl_groups(groups),
			RedrawNotification::OptionSet(options) => {
				for option in options {
					self.set_option(option);
				}
			},
			RedrawNotification::ModeInfoSet(sets) => {
				if let Some(set) = sets.into_iter().last() {
					self.cursor.set_modes(set.cursor_style_enabled, set.mode_info);
					self.stale = true;
				}
			},
			RedrawNotification::ModeChange(changes) => {
				if let Some(change) = changes.last() {
					self.cursor.set_mode(change.mode_idx as usize, &change.mode);
					self.stale = true;
				}
			},
			RedrawNotification::BusyStart => {
				self.cursor.set_hidden(true);
				self.stale = true;
			},
			RedrawNotification::BusyStop => {
				self.cursor.set_hidden(false);
				self.stale = true;
			},
			RedrawNotification::MouseOn => self.mouse = true,
			RedrawNotification::MouseOff => self.mouse = false,
			RedrawNotification::Flush => return self.flush(),
			_ => (),
		}
		None
	}

	/// Forgets `grid` and its window's styles, place and fonts (`grid_destroy`).
	fn destroy_grid(&mut self, grid: Grid) {
		self.grids.remove(&grid);
		if let Some(win) = self.viewports.remove(&grid).and_then(|viewport| viewport.win) {
			self.scroll_animations.remove(&win);
		}
		match self.placements.remove(&grid) {
			Some(Placement::Float { win, .. }) => _ = self.floats.remove(&win),
			Some(Placement::External { win, .. }) => {
				self.places.remove(&win);
				self.external_fonts.remove(&win);
			},
			_ => (),
		}
		self.stale = true;
	}

	/// Shows `grid` as external window `win` (`win_external_pos`). Sent again on resizes and tab switches: the window
	/// keeps its place in the stack.
	fn place_external(&mut self, grid: Grid, win: Window) {
		let order = match self.placements.get(&grid) {
			Some(Placement::External { order, .. }) => *order,
			_ => {
				self.external_order += 1;
				self.external_order
			},
		};
		self.place(grid, Placement::External { win, order });
	}

	/// Adds a scroll of a window's text to the next redraw's (`win_viewport`). When the line count changes, the window
	/// likely shows another buffer, which Neovim also reports as a scroll: that doesn't slide.
	fn set_viewport(&mut self, event: WinViewportEvent) {
		let viewport = self.viewports.entry(event.grid).or_default();
		let same_buffer = viewport.line_count == event.line_count;
		viewport.scrolled = if same_buffer { viewport.scrolled + event.scroll_delta } else { 0 };
		viewport.line_count = event.line_count;
		viewport.win = Some(event.win);
	}

	/// Keeps the options that change what's drawn (`option_set`): other fonts or line space are drawn with the next
	/// flush.
	fn set_option(&mut self, option: OptionSetEvent) {
		match (option.name.as_str(), option.value) {
			("guifont", OptionValue::String(value)) => {
				// Neovim sends its default with the options at attach, before the user's config can set one.
				let default = self.default_guifont.get_or_insert_with(|| value.clone());
				let value = if value == *default { String::new() } else { value };
				if value != self.guifont {
					self.guifont = value;
					self.stale = true;
				}
			},
			("linespace", OptionValue::Number(value)) if value != self.linespace => {
				self.linespace = value;
				self.stale = true;
			},
			("mousemoveevent", OptionValue::Bool(value)) => self.mouse_move_events = value,
			_ => (),
		}
	}

	/// Applies `change` to grid `id`, to be drawn on the next `flush`. `event` names the event for the warning about
	/// an unknown grid.
	fn update_grid(&mut self, id: Grid, event: &str, change: impl FnOnce(&mut GridState)) {
		let Some(grid) = self.grids.get_mut(&id) else {
			tracing::warn!("{event}: unknown grid {id}");
			return;
		};
		change(grid);
		self.stale = true;
	}

	fn place(&mut self, grid: Grid, placement: Placement) {
		self.placements.insert(grid, placement);
		self.stale = true;
	}

	/// The layers to draw, if anything changed since the last `flush`.
	pub(crate) fn flush(&mut self) -> Option<Redraw> {
		let mut redraw =
			std::mem::take(&mut self.stale).then(|| Redraw { background: self.background(), layers: self.layers() });
		if let Some(redraw) = &mut redraw {
			self.move_cursor(&mut redraw.layers);
		}
		// A scroll belongs to the redraw it's part of.
		for viewport in self.viewports.values_mut() {
			viewport.scrolled = 0;
		}
		redraw
	}

	/// Gives the cursor among `layers`, the first one's as the renderer draws it, how it moves there, and notes where
	/// it's drawn.
	fn move_cursor(&mut self, layers: &mut [Layer]) {
		let found = layers.iter_mut().find_map(|layer| {
			let (id, cell, x, y) = (layer.id, layer.cell, layer.x, layer.y);
			Some((id, cell, [x, y], layer.cursor.as_mut()?))
		});
		let Some((id, cell, [x, y], cursor)) = found else {
			return;
		};
		let at = [x + cursor.col * cell.width, y + cursor.row * cell.height];
		// The caret in what an input method composes moves at once.
		if id != PREEDIT_ID {
			cursor.motion = self.cursor.motion_to(at, cell);
		}
		self.cursor.drawn(at);
	}
}
