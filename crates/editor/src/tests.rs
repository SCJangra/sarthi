use renderer::{CellSize, Cursor, CursorShape, Edges, FontId, Layer, Motion, Shadow};
use rpc::neovim::{
	CellText, DefaultColorsSetEvent, Grid, GridCell, GridCursorGotoEvent, GridDestroyEvent, GridLineEvent,
	GridResizeEvent, HlAttrDefineEvent, HlAttrs, HlGroupSetEvent, HlId, MsgSetPosEvent, OptionSetEvent, OptionValue,
	RedrawNotification, WinCloseEvent, WinExternalPosEvent, WinFloatPosEvent, WinHideEvent, WinPosEvent,
	WinViewportEvent, WinViewportMarginsEvent, Window,
};
use serde::de::IgnoredAny;

use super::{Anchor, Editor, FloatStyle, GridPosition, OutlineStyle, Place, Pointer, Preedit, Redraw, Relative};

const FG: u32 = 0xC7C7C7;
const BG: u32 = 0x1C1C1C;
const CELL: CellSize = CellSize { width: 8, height: 16 };

fn resize(grid: u32, width: u32, height: u32) -> RedrawNotification {
	RedrawNotification::GridResize(vec![GridResizeEvent { grid: Grid(grid), width, height }])
}

fn line(grid: u32, row: u32, text: &str) -> RedrawNotification {
	let cells = text
		.chars()
		.map(|c| GridCell { text: CellText::from(&c.to_string()).unwrap(), hl_id: None, repeat: None })
		.collect();
	RedrawNotification::GridLine(vec![GridLineEvent { grid: Grid(grid), row, col_start: 0, cells, wrap: false }])
}

fn goto(grid: u32, row: u32, col: u32) -> RedrawNotification {
	RedrawNotification::GridCursorGoto(vec![GridCursorGotoEvent { grid: Grid(grid), row, col }])
}

fn colors() -> RedrawNotification {
	RedrawNotification::DefaultColorsSet(vec![DefaultColorsSetEvent {
		rgb_fg: FG,
		rgb_bg: BG,
		rgb_sp: 0xFF0000,
		cterm_fg: 0,
		cterm_bg: 0,
	}])
}

/// The text of each row of the only layer of `redraw`
fn rows(redraw: &Redraw) -> Vec<String> {
	let [layer] = redraw.layers.as_slice() else { panic!("expected one layer") };
	layer
		.cells
		.chunks(layer.width as usize)
		.map(|row| row.iter().map(|cell| cell.text.as_str()).collect())
		.collect()
}

#[test]
fn draws_only_when_a_redraw_ends() {
	let mut editor = Editor::new();
	assert_eq!(editor.apply(vec![colors(), resize(1, 3, 2), line(1, 0, "abc")]), None);

	let redraw = editor.apply(vec![line(1, 1, "def"), RedrawNotification::Flush]).unwrap();
	assert_eq!(redraw.background, BG);
	assert_eq!(rows(&redraw), ["abc", "def"]);
}

#[test]
fn nothing_to_draw_without_changes() {
	let mut editor = Editor::new();
	assert!(editor.apply(vec![resize(1, 2, 1), RedrawNotification::Flush]).is_some());
	assert_eq!(editor.apply(vec![RedrawNotification::Flush]), None);
	// An unknown grid changes nothing.
	assert_eq!(editor.apply(vec![line(7, 0, "x"), RedrawNotification::Flush]), None);
}

#[test]
fn the_last_redraw_of_a_batch_wins() {
	let mut editor = Editor::new();
	let events =
		vec![resize(1, 1, 1), line(1, 0, "a"), RedrawNotification::Flush, line(1, 0, "b"), RedrawNotification::Flush];
	assert_eq!(rows(&editor.apply(events).unwrap()), ["b"]);
}

#[test]
fn cursor_is_drawn_where_neovim_put_it() {
	let mut editor = Editor::new();
	let events = vec![colors(), resize(1, 2, 1), line(1, 0, "ab"), goto(1, 0, 1), RedrawNotification::Flush];
	let redraw = editor.apply(events).unwrap();
	let [layer] = redraw.layers.as_slice() else { panic!("expected one layer") };
	// A block cursor in the cell's swapped colors, on "b"
	let cursor =
		Cursor { row: 0, col: 1, width: 1, shape: CursorShape::Block, color: FG, text_color: BG, animate: true };
	assert_eq!(layer.cursor, Some(cursor));

	// Moving the cursor is a change to draw.
	assert!(editor.apply(vec![goto(1, 0, 0), RedrawNotification::Flush]).is_some());
}

#[test]
fn an_input_methods_text_is_drawn_over_the_cursor_until_committed() {
	let mut editor = Editor::new();
	editor.apply(vec![colors(), resize(1, 4, 1), line(1, 0, "abcd"), goto(1, 0, 1), RedrawNotification::Flush]);

	editor.set_preedit(Some(Preedit { text: String::from("xy"), caret: None }));
	let redraw = editor.refresh().unwrap();
	let [grid, preedit] = redraw.layers.as_slice() else { panic!("expected two layers") };
	assert_eq!((grid.cursor, preedit.x, preedit.width), (None, grid.cell.width, 2));

	// The same text again changes nothing; none, once committed, leaves the grid as it was.
	editor.set_preedit(Some(Preedit { text: String::from("xy"), caret: None }));
	assert_eq!(editor.refresh(), None);
	editor.set_preedit(None);
	let redraw = editor.refresh().unwrap();
	assert_eq!(redraw.layers.len(), 1);
}

fn win_pos(grid: u32, row: u32, col: u32) -> RedrawNotification {
	let position =
		WinPosEvent { grid: Grid(grid), win: IgnoredAny, start_row: row, start_col: col, width: 0, height: 0 };
	RedrawNotification::WinPos(vec![position])
}

fn external(grid: u32) -> RedrawNotification {
	RedrawNotification::WinExternalPos(vec![WinExternalPosEvent { grid: Grid(grid), win: win_of(grid) }])
}

fn float(grid: u32, row: u32, col: u32, compindex: u32) -> RedrawNotification {
	float_at_level(grid, row, col, compindex, 50)
}

/// Like [`float`], at `zindex`
fn float_at_level(grid: u32, row: u32, col: u32, compindex: u32, zindex: u32) -> RedrawNotification {
	RedrawNotification::WinFloatPos(vec![WinFloatPosEvent {
		grid: Grid(grid),
		win: win_of(grid),
		anchor: String::from("NW"),
		anchor_grid: Grid(1),
		anchor_row: f64::from(row),
		anchor_col: f64::from(col),
		mouse_enabled: true,
		zindex,
		compindex,
		screen_row: row,
		screen_col: col,
	}])
}

fn messages(row: u32, scrolled: bool, compindex: u32) -> RedrawNotification {
	let sep_char = String::from("─");
	RedrawNotification::MsgSetPos(vec![MsgSetPosEvent {
		grid: Grid(3),
		row,
		scrolled,
		sep_char,
		zindex: 200,
		compindex,
	}])
}

/// The middle of screen cell `row`, `col`, in pixels
fn middle(row: u32, col: u32) -> (f64, f64) {
	((f64::from(col) + 0.5) * f64::from(CELL.width), (f64::from(row) + 0.5) * f64::from(CELL.height))
}

/// Where each layer of `redraw` is, in cells of the screen: (id, row, col, height)
fn placement(redraw: &Redraw) -> Vec<(u64, u32, u32, u32)> {
	redraw
		.layers
		.iter()
		.map(|layer| (layer.id, layer.y / CELL.height, layer.x / CELL.width, layer.height))
		.collect()
}

fn flush(editor: &mut Editor, mut events: Vec<RedrawNotification>) -> Redraw {
	events.push(RedrawNotification::Flush);
	editor.apply(events).expect("something changed")
}

/// An editor with a 10×5 screen (grid 1) of [`CELL`]s and a 4×3 window grid 2 that isn't placed yet
fn screen() -> Editor {
	let mut editor = Editor::new();
	editor.set_cell_size(CELL);
	editor.apply(vec![colors(), resize(1, 10, 5), resize(2, 4, 3)]);
	editor
}

#[test]
fn windows_are_drawn_once_placed() {
	let mut editor = screen();
	assert_eq!(placement(&flush(&mut editor, vec![])), [(1, 0, 0, 5)]);
	assert_eq!(placement(&flush(&mut editor, vec![win_pos(2, 0, 6)])), [(1, 0, 0, 5), (2, 0, 6, 3)]);
}

#[test]
fn floats_come_after_windows_in_neovims_order() {
	let mut editor = screen();
	let events = vec![
		resize(6, 3, 2),
		resize(7, 3, 2),
		resize(9, 2, 2),
		float(6, 1, 1, 2),
		float(7, 2, 2, 1),
		win_pos(9, 3, 0),
		win_pos(2, 0, 6),
	];
	let ids: Vec<u64> = placement(&flush(&mut editor, events)).iter().map(|(id, ..)| *id).collect();
	assert_eq!(ids, [1, 2, 9, 7, 6]);
}

#[test]
fn message_grid_shows_from_its_row_down() {
	let mut editor = screen();
	// Like Neovim, the message grid is as large as the screen.
	editor.apply(vec![resize(3, 10, 5), line(3, 0, "hello")]);
	assert_eq!(placement(&flush(&mut editor, vec![messages(4, false, 1)])), [(1, 0, 0, 5), (3, 4, 0, 1)]);

	// Scrolled up over the windows: a separator line in MsgSeparator above the messages
	let separator = HlAttrs { foreground: Some(0x00FF00), ..crate::highlight::tests::attrs() };
	let events = vec![
		RedrawNotification::HlAttrDefine(vec![HlAttrDefineEvent {
			id: HlId(9),
			rgb_attr: separator.clone(),
			cterm_attr: separator,
			info: vec![],
		}]),
		RedrawNotification::HlGroupSet(vec![HlGroupSetEvent { name: String::from("MsgSeparator"), hl_id: HlId(9) }]),
		messages(2, true, 1),
	];
	let redraw = flush(&mut editor, events);
	let separator_id = u64::from(u32::MAX) + 1;
	assert_eq!(placement(&redraw), [(1, 0, 0, 5), (separator_id, 1, 0, 1), (3, 2, 0, 3)]);

	let Layer { cells, .. } = &redraw.layers[1];
	assert!(
		cells
			.iter()
			.all(|cell| cell.text.as_str() == "─" && cell.style.foreground == 0x00FF00)
	);
	assert_eq!(cells.len(), 10);
}

#[test]
fn hidden_closed_and_destroyed_grids() {
	let mut editor = screen();
	flush(&mut editor, vec![win_pos(2, 0, 6)]);

	let hide = RedrawNotification::WinHide(vec![WinHideEvent { grid: Grid(2) }]);
	assert_eq!(placement(&flush(&mut editor, vec![hide])), [(1, 0, 0, 5)]);
	assert_eq!(placement(&flush(&mut editor, vec![win_pos(2, 1, 6)])), [(1, 0, 0, 5), (2, 1, 6, 3)]);

	let close = RedrawNotification::WinClose(vec![WinCloseEvent { grid: Grid(2) }]);
	let destroy = RedrawNotification::GridDestroy(vec![GridDestroyEvent { grid: Grid(2) }]);
	assert_eq!(placement(&flush(&mut editor, vec![close, destroy])), [(1, 0, 0, 5)]);
	// A destroyed grid takes events no more.
	assert_eq!(editor.apply(vec![line(2, 0, "x"), RedrawNotification::Flush]), None);
}

#[test]
fn cursor_is_drawn_in_its_window() {
	let mut editor = screen();
	let redraw = flush(&mut editor, vec![win_pos(2, 0, 6), line(2, 0, "ab"), goto(2, 0, 1)]);
	let cursor = |index: usize| {
		let layer = &redraw.layers[index];
		layer.cursor.map(|cursor| (cursor.row, cursor.col))
	};
	// On "b" in the window, and not in grid 1
	assert_eq!((cursor(0), cursor(1)), (None, Some((0, 1))));
}

/// The window of grid `grid`, in these tests
fn win_of(grid: u32) -> Window {
	Window(1000 + i64::from(grid))
}

fn viewport(grid: u32, line_count: u32, scroll_delta: i64) -> RedrawNotification {
	RedrawNotification::WinViewport(vec![WinViewportEvent {
		grid: Grid(grid),
		win: win_of(grid),
		topline: 0,
		botline: 0,
		curline: 0,
		curcol: 0,
		line_count,
		scroll_delta,
	}])
}

/// The scroll of grid `id`'s layer in `redraw`: (rows, top, bottom, left, right)
fn scroll_of(redraw: &Redraw, id: u64) -> Option<(i32, u32, u32, u32, u32)> {
	let layer = redraw.layers.iter().find(|layer| layer.id == id)?;
	layer
		.scroll
		.map(|scroll| (scroll.rows, scroll.top, scroll.bottom, scroll.left, scroll.right))
}

#[test]
fn a_scroll_is_part_of_its_redraw() {
	let mut editor = screen();
	flush(&mut editor, vec![win_pos(2, 0, 0), viewport(2, 100, 0)]);

	// Scrolls add up until the redraw ends.
	let redraw = flush(&mut editor, vec![viewport(2, 100, 2), viewport(2, 100, 1), line(2, 0, "a")]);
	assert_eq!(scroll_of(&redraw, 2), Some((3, 0, 3, 0, 4)));
	assert_eq!(scroll_of(&redraw, 1), None);

	// The next redraw didn't scroll.
	assert_eq!(scroll_of(&flush(&mut editor, vec![line(2, 0, "b")]), 2), None);

	// A redraw without changes drops its scroll, so the next one doesn't slide.
	editor.apply(vec![viewport(2, 100, 1), RedrawNotification::Flush]);
	assert_eq!(scroll_of(&flush(&mut editor, vec![line(2, 0, "c")]), 2), None);
}

#[test]
fn margins_dont_scroll() {
	let mut editor = screen();
	let margins = WinViewportMarginsEvent { grid: Grid(2), win: IgnoredAny, top: 1, bottom: 0, left: 1, right: 1 };
	flush(
		&mut editor,
		vec![win_pos(2, 0, 0), RedrawNotification::WinViewportMargins(vec![margins]), viewport(2, 9, 0)],
	);
	let redraw = flush(&mut editor, vec![viewport(2, 9, -1), line(2, 1, "a")]);
	assert_eq!(scroll_of(&redraw, 2), Some((-1, 1, 3, 1, 3)));
}

#[test]
fn another_buffer_doesnt_scroll() {
	let mut editor = screen();
	// Neovim reports the first viewport of a window, and switching buffers, as scrolls.
	let redraw = flush(&mut editor, vec![win_pos(2, 0, 0), viewport(2, 500, 243)]);
	assert_eq!(scroll_of(&redraw, 2), None);
	let redraw = flush(&mut editor, vec![viewport(2, 300, -294), line(2, 0, "a")]);
	assert_eq!(scroll_of(&redraw, 2), None);
}

#[test]
fn a_see_through_window_shows_through_the_default_background() {
	let blends = |redraw: &Redraw, id: u64| {
		let layer = redraw.layers.iter().find(|layer| layer.id == id).unwrap();
		layer.cells.iter().map(|cell| cell.blend).collect::<Vec<_>>()
	};
	let mut editor = screen();
	flush(&mut editor, vec![resize(3, 2, 1), win_pos(2, 0, 0), float(3, 1, 1, 1)]);
	editor.set_see_through(true);
	// In the window, a cell with its own background stays opaque.
	let attrs = crate::highlight::tests::attrs;
	let rgb_attr = HlAttrs { background: Some(0x123456), ..attrs() };
	let colored = HlAttrDefineEvent { id: HlId(1), rgb_attr, cterm_attr: attrs(), info: vec![] };
	let cell = GridCell { text: CellText::from("a").unwrap(), hl_id: Some(HlId(1)), repeat: None };
	let line = GridLineEvent { grid: Grid(2), row: 0, col_start: 0, cells: vec![cell], wrap: false };
	let events = vec![RedrawNotification::HlAttrDefine(vec![colored]), RedrawNotification::GridLine(vec![line])];
	let redraw = flush(&mut editor, events);

	assert_eq!(&blends(&redraw, 2)[..2], [0, 100]);
	assert!(blends(&redraw, 1).iter().all(|blend| *blend == 100));
	// The float keeps its background.
	assert_eq!(blends(&redraw, 3), [0, 0]);

	editor.set_see_through(false);
	assert!(blends(&flush(&mut editor, vec![]), 1).iter().all(|blend| *blend == 0));
}

#[test]
fn a_see_through_window_hides_what_grid_1_has_under_it() {
	// Neovim leaves an old separator in grid 1, where window 2 now is.
	let mut editor = screen();
	flush(&mut editor, vec![line(1, 0, "  |"), line(1, 4, "  |"), win_pos(2, 0, 0)]);
	let grid_1 = |redraw: &Redraw| {
		let layer = redraw.layers.iter().find(|layer| layer.id == 1).unwrap();
		layer
			.cells
			.iter()
			.take(3)
			.map(|cell| (cell.text.as_str().to_owned(), cell.blend))
			.collect::<Vec<_>>()
	};

	// Opaque, the window hides it.
	let redraw = flush(&mut editor, vec![line(2, 0, "a")]);
	assert_eq!(grid_1(&redraw)[2], (String::from("|"), 0));

	// See-through, grid 1 is blank under the window: only the window shows there.
	editor.set_see_through(true);
	let redraw = flush(&mut editor, vec![]);
	assert_eq!(grid_1(&redraw)[2], (String::from(" "), 100));
	// Outside the window, grid 1 keeps what it shows.
	let layer = redraw.layers.iter().find(|layer| layer.id == 1).unwrap();
	assert_eq!(layer.cells[4 * 10 + 2].text.as_str(), "|");
}

#[test]
fn the_pointer_follows_whats_under_it() {
	// A 4×3 window at the top-left of the 10×5 screen: its separator is column 4, its status line row 3.
	let mut editor = screen();
	flush(&mut editor, vec![win_pos(2, 0, 0)]);
	let (x, y) = middle(0, 0);
	assert_eq!(editor.pointer_at(x, y), Pointer::Arrow, "the mouse is off");

	editor.apply(vec![RedrawNotification::MouseOn]);
	let at = |row, col| {
		let (x, y) = middle(row, col);
		editor.pointer_at(x, y)
	};
	assert_eq!((at(0, 0), at(2, 3)), (Pointer::Text, Pointer::Text));
	assert_eq!((at(0, 4), at(2, 4)), (Pointer::ResizeColumns, Pointer::ResizeColumns));
	assert_eq!((at(3, 0), at(3, 3)), (Pointer::ResizeRows, Pointer::ResizeRows));
	// Where they meet, and away from both
	assert_eq!((at(3, 4), at(4, 8), at(1, 7)), (Pointer::ResizeRows, Pointer::Arrow, Pointer::Arrow));
}

#[test]
fn external_windows_are_centered_over_everything() {
	let mut editor = screen();
	editor.set_default_float_style(FloatStyle {
		shadow: Some(Shadow { blur: 8.0, offset: [0.0, 0.0], color: 0, opacity: 0.5 }),
		..FloatStyle::default()
	});
	// A float and a 4×2 external window: the screen is 80×80 pixels, the window 32×32.
	let events = vec![resize(3, 2, 1), float(3, 0, 0, 5), resize(4, 4, 2), external(4)];
	let redraw = flush(&mut editor, events);
	let ids: Vec<u64> = redraw.layers.iter().map(|layer| layer.id).collect();
	assert_eq!(ids, [1, 3, 4]);
	let window = &redraw.layers[2];
	assert_eq!((window.x, window.y), (24, 24));
	// Its own shadow, under it
	assert_eq!(shadow_of(&redraw, 4), Some(vec![[3, 1, 4, 2]]));

	// A larger window is centered again.
	let redraw = flush(&mut editor, vec![resize(4, 8, 4), external(4)]);
	let window = &redraw.layers[2];
	assert_eq!((window.x, window.y), (8, 8));
}

#[test]
fn external_windows_go_where_placed() {
	let mut editor = screen();
	let at = |redraw: &Redraw| {
		let layer = redraw.layers.iter().find(|layer| layer.id == 4).unwrap();
		(layer.x, layer.y)
	};
	// A 2×1 external window a row below window 2's top-left corner
	flush(&mut editor, vec![win_pos(2, 1, 6), viewport(2, 10, 0), resize(4, 2, 1), external(4)]);
	let below = Place::Relative {
		relative: Relative::Win(win_of(2)),
		anchor: Anchor::NorthWest,
		row: 1.0,
		col: 0.0,
		offset: [0.0; 2],
		fixed: false,
	};
	editor.set_external_place(win_of(4), below);
	assert_eq!(at(&editor.refresh().expect("placed")), (6 * 8, 2 * 16));

	// It follows the window.
	assert_eq!(at(&flush(&mut editor, vec![win_pos(2, 2, 0)])), (0, 3 * 16));

	// Placed by the cursor, it goes where the cursor is, and stays there.
	flush(&mut editor, vec![goto(1, 3, 4)]);
	let cursor = Place::Relative {
		relative: Relative::Cursor,
		anchor: Anchor::NorthWest,
		row: 1.0,
		col: 0.0,
		offset: [0.0; 2],
		fixed: false,
	};
	editor.set_external_place(win_of(4), cursor);
	assert_eq!(at(&editor.refresh().expect("placed")), (4 * 8, 4 * 16));
	assert_eq!(at(&flush(&mut editor, vec![goto(1, 0, 0)])), (4 * 8, 4 * 16));

	// Relative to external window 5, at the top left with cells of its own (10×20): in those cells
	let events = vec![viewport(5, 10, 0), resize(5, 2, 1), external(5)];
	flush(&mut editor, events);
	editor.set_external_font(win_of(5), Some((FontId::MAIN, CellSize { width: 10, height: 20 })));
	let top_left = Place::Relative {
		relative: Relative::Editor,
		anchor: Anchor::NorthWest,
		row: 0.0,
		col: 0.0,
		offset: [0.0; 2],
		fixed: false,
	};
	editor.set_external_place(win_of(5), top_left);
	let below = Place::Relative {
		relative: Relative::Win(win_of(5)),
		anchor: Anchor::NorthWest,
		row: 1.0,
		col: 2.0,
		offset: [0.0; 2],
		fixed: false,
	};
	editor.set_external_place(win_of(4), below);
	assert_eq!(at(&editor.refresh().expect("placed")), (2 * 10, 20));
	// Counted from 5's text, inside its padding, like from inside a float's border
	editor.set_float_style(win_of(5), FloatStyle { padding: Some(Edges::all(4.0)), ..FloatStyle::default() });
	assert_eq!(at(&editor.refresh().expect("padded")), (2 * 10 + 4, 20 + 4));

	// Placed relative to each other: no loop, centered past a few
	editor.set_external_place(
		win_of(5),
		Place::Relative {
			relative: Relative::Win(win_of(4)),
			anchor: Anchor::NorthWest,
			row: 0.0,
			col: 0.0,
			offset: [0.0; 2],
			fixed: false,
		},
	);
	let _ = editor.refresh();
}

#[test]
fn the_mouse_over_an_external_window() {
	let mut editor = screen();
	// A 4×2 external window, centered at (24, 24) on the 80×80 pixel screen, over window grid 2
	flush(&mut editor, vec![win_pos(2, 0, 0), resize(4, 4, 2), external(4), RedrawNotification::MouseOn]);
	let cell = |grid, row, col| Some(GridPosition { grid: Grid(grid), row, col });
	assert_eq!(editor.grid_at(24.0, 24.0), cell(4, 0, 0));
	assert_eq!(editor.grid_at(24.0 + 3.0 * 8.0 + 1.0, 24.0 + 16.0 + 1.0), cell(4, 1, 3));
	assert_eq!(editor.pointer_at(30.0, 30.0), Pointer::Text);
	// Next to it, what's below it
	assert_eq!(editor.grid_at(23.0, 24.0), cell(2, 1, 2));

	// Its cells reach past it, e.g. for a drag: two rows and three columns before its top-left corner
	assert_eq!(editor.grid_cell(Grid(4), 0.0, 0.0), Some((-2, -3)));
	assert_eq!(editor.grid_cell(Grid(2), 8.0, 16.0), Some((1, 1)));
	assert_eq!(editor.grid_cell(Grid(9), 0.0, 0.0), None);
}

#[test]
fn an_external_window_in_fonts_of_its_own() {
	let mut editor = screen();
	flush(&mut editor, vec![resize(4, 4, 2), external(4)]);
	// Cells of 10×20 pixels: 40×40 for its 4×2 cells, centered on the 80×80 pixel screen
	let cell = CellSize { width: 10, height: 20 };
	editor.set_external_font(win_of(4), Some((FontId::MAIN, cell)));
	assert_eq!(editor.external_fonts().collect::<Vec<_>>(), [win_of(4)]);
	let redraw = editor.refresh().expect("its cells changed");
	let window = redraw.layers.iter().find(|layer| layer.id == 4).unwrap();
	assert_eq!((window.x, window.y, window.cell), (20, 20, cell));
	// The mouse goes by its cells too.
	assert_eq!(editor.grid_at(20.0 + 10.0 + 1.0, 20.0 + 1.0), Some(GridPosition { grid: Grid(4), row: 0, col: 1 }));

	// Back to the main fonts
	editor.set_external_font(win_of(4), None);
	let redraw = editor.refresh().expect("its cells changed");
	let window = redraw.layers.iter().find(|layer| layer.id == 4).unwrap();
	assert_eq!(window.cell, CELL);
}

#[test]
fn external_windows_keep_their_order() {
	let mut editor = screen();
	let events = vec![resize(4, 2, 1), resize(5, 2, 1), external(4), external(5)];
	flush(&mut editor, events);
	// Neovim sends the first again, e.g. after a tab switch: it stays under the second.
	let redraw = flush(&mut editor, vec![external(4)]);
	let ids: Vec<u64> = redraw.layers.iter().map(|layer| layer.id).collect();
	assert_eq!(ids, [1, 4, 5]);

	// Closed, it's gone.
	let closed = RedrawNotification::WinClose(vec![WinCloseEvent { grid: Grid(4) }]);
	let redraw = flush(&mut editor, vec![closed]);
	assert_eq!(redraw.layers.len(), 2);
}

#[test]
fn layers_are_placed_in_pixels() {
	let mut editor = screen();
	let redraw = flush(&mut editor, vec![win_pos(2, 1, 6)]);
	let window = &redraw.layers[1];
	assert_eq!((window.x, window.y, window.cell), (6 * 8, 16, CELL));

	// A new cell size moves and resizes them.
	let cell = CellSize { width: 10, height: 20 };
	editor.set_cell_size(cell);
	let redraw = editor.refresh().expect("the cell size changed");
	let window = &redraw.layers[1];
	assert_eq!((window.x, window.y, window.cell), (60, 20, cell));
	assert_eq!(editor.refresh(), None, "nothing changed since");
}

#[test]
fn outlines_around_external_windows() {
	let mut editor = screen();
	let outline_of = |redraw: &Redraw, id: u64| {
		let layer = redraw.layers.iter().find(|layer| layer.id == id).unwrap();
		layer.outline
	};
	// A float and an external window; none until a style has one
	let events = vec![resize(3, 2, 1), float(3, 0, 0, 1), resize(4, 2, 1), external(4)];
	let redraw = flush(&mut editor, events);
	assert_eq!((outline_of(&redraw, 3), outline_of(&redraw, 4)), (None, None));

	// In the highlight group's text color, here the default one, in physical pixels
	let outline = |width| Some(OutlineStyle { width, highlight: String::from("FloatBorder"), color: None });
	editor.set_default_float_style(FloatStyle { outline: outline(2.0), ..FloatStyle::default() });
	editor.set_scale(1.5);
	let redraw = editor.refresh().expect("outlines changed");
	let drawn = Some(renderer::Outline { width: 3.0, color: FG });
	// Never around floats, which Neovim places without room for it, tiled windows or the screen
	assert_eq!((outline_of(&redraw, 3), outline_of(&redraw, 4)), (None, drawn));
	assert_eq!(outline_of(&redraw, 1), None);

	// A window's own style: one of its own, or none; a float's own still none
	editor.set_float_style(win_of(3), FloatStyle { outline: outline(3.0), ..FloatStyle::default() });
	editor.set_float_style(win_of(4), FloatStyle { outline: outline(3.0), ..FloatStyle::default() });
	let redraw = editor.refresh().expect("their styles changed");
	let thick = Some(renderer::Outline { width: 4.5, color: FG });
	assert_eq!((outline_of(&redraw, 3), outline_of(&redraw, 4)), (None, thick));
	editor.set_float_style(win_of(4), FloatStyle::default());
	assert_eq!(outline_of(&editor.refresh().expect("its style changed"), 4), None);

	// In the color Lua found for its group, which the editor may not know
	let green = OutlineStyle { width: 1.0, highlight: String::from("Comment"), color: Some(0x00FF00) };
	editor.set_float_style(win_of(4), FloatStyle { outline: Some(green), ..FloatStyle::default() });
	let redraw = editor.refresh().expect("its style changed");
	assert_eq!(outline_of(&redraw, 4), Some(renderer::Outline { width: 1.5, color: 0x00FF00 }));
}

#[test]
fn rounded_corners_of_external_windows() {
	let mut editor = screen();
	let shadow = Some(Shadow { blur: 8.0, offset: [0.0, 0.0], color: 0, opacity: 0.5 });
	editor.set_default_float_style(FloatStyle { shadow, ..FloatStyle::default() });
	let radius_of = |redraw: &Redraw, id: u64| {
		let layer = redraw.layers.iter().find(|layer| layer.id == id).unwrap();
		(layer.corner_radius, layer.shadow.as_ref().map(|shadow| shadow.radius))
	};
	// A lone float, two floats side by side at another level, and an external window
	let events = vec![
		resize(3, 2, 1),
		float_at_level(3, 0, 0, 1, 50),
		resize(5, 2, 1),
		float_at_level(5, 2, 0, 2, 60),
		resize(6, 2, 1),
		float_at_level(6, 2, 2, 3, 60),
		resize(4, 2, 1),
		external(4),
	];
	let redraw = flush(&mut editor, events);
	assert_eq!(radius_of(&redraw, 3), (0.0, Some(0.0)), "square until a style rounds them");

	editor.set_default_float_style(FloatStyle { shadow, corners: Some(3.0), ..FloatStyle::default() });
	editor.set_scale(2.0);
	let redraw = editor.refresh().expect("corners changed");
	// In physical pixels, and the shadow too
	assert_eq!(radius_of(&redraw, 4), (6.0, Some(6.0)));
	// Never floats', which Neovim places in its cells, alone or side by side, nor the screen's or tiled windows'
	assert_eq!(radius_of(&redraw, 3), (0.0, Some(0.0)));
	assert_eq!((radius_of(&redraw, 5), radius_of(&redraw, 6)), ((0.0, Some(0.0)), (0.0, None)));
	assert_eq!(radius_of(&redraw, 1).0, 0.0);

	// A window's own radius, with the shadow round its outline
	let outline = Some(OutlineStyle { width: 1.0, highlight: String::from("FloatBorder"), color: None });
	editor.set_float_style(
		win_of(4),
		FloatStyle { shadow, outline, corners: Some(5.0), padding: None, parent: None, ..FloatStyle::default() },
	);
	let redraw = editor.refresh().expect("their styles changed");
	assert_eq!(radius_of(&redraw, 4), (10.0, Some(12.0)));
}

#[test]
fn padding_around_external_windows() {
	let mut editor = screen();
	let shadow = Some(Shadow { blur: 8.0, offset: [0.0, 0.0], color: 0, opacity: 0.5 });
	editor.set_default_float_style(FloatStyle { shadow, ..FloatStyle::default() });
	let padding_of = |redraw: &Redraw, id: u64| {
		let layer = redraw.layers.iter().find(|layer| layer.id == id).unwrap();
		let area = layer
			.shadow
			.as_ref()
			.map(|shadow| shadow.area.iter().map(|&rect| <[u32; 4]>::from(rect)).collect::<Vec<_>>());
		(<[u32; 4]>::from(layer.padding), area)
	};
	// A float at row 1, column 1, and an external window at the top left; none until a style has it
	let events = vec![resize(3, 2, 1), float(3, 1, 1, 1), resize(4, 2, 1), external(4)];
	flush(&mut editor, events);
	let place = Place::Relative {
		relative: Relative::Editor,
		anchor: Anchor::NorthWest,
		row: 0.0,
		col: 0.0,
		offset: [0.0; 2],
		fixed: false,
	};
	editor.set_external_place(win_of(4), place);
	let redraw = editor.refresh().expect("placed");
	assert_eq!(padding_of(&redraw, 3), ([0; 4], Some(vec![[8, 16, 16, 16]])));
	assert_eq!(padding_of(&redraw, 4), ([0; 4], Some(vec![[0, 0, 16, 16]])));

	// In whole physical pixels; the shadow is cast from around it, and the outline around that.
	let outline = Some(OutlineStyle { width: 1.0, highlight: String::from("FloatBorder"), color: None });
	let style = FloatStyle { shadow, outline, padding: Some(Edges::all(3.0)), ..FloatStyle::default() };
	editor.set_default_float_style(style.clone());
	editor.set_scale(2.0);
	let redraw = editor.refresh().expect("padding changed");
	// The external window's cells move 8 pixels away from its anchor (NW), so its frame starts where it's placed.
	assert_eq!(padding_of(&redraw, 4), ([6; 4], Some(vec![[0, 0, 32, 32]])));
	// Never a float's, which Neovim places without room for it, nor the screen's or tiled windows'
	assert_eq!(padding_of(&redraw, 3), ([0; 4], Some(vec![[8, 16, 16, 16]])));
	assert_eq!(padding_of(&redraw, 1).0, [0; 4]);

	// A window's own size, or none
	editor.set_float_style(win_of(4), FloatStyle { padding: Some(Edges::all(1.0)), ..style.clone() });
	let redraw = editor.refresh().expect("its style changed");
	assert_eq!(padding_of(&redraw, 4).0, [2; 4]);

	// Each side its own: top 1, right 2, bottom 3, left 4, so 2, 4, 6 and 8 pixels, and the shadow grows by each
	// side's.
	editor.set_float_style(win_of(4), FloatStyle { padding: Some(Edges::from([1.0, 2.0, 3.0, 4.0])), ..style });
	let redraw = editor.refresh().expect("its padding changed");
	assert_eq!(padding_of(&redraw, 4), ([2, 4, 6, 8], Some(vec![[0, 0, 32, 28]])));
	editor.set_float_style(win_of(4), FloatStyle { shadow, ..FloatStyle::default() });
	assert_eq!(padding_of(&editor.refresh().expect("none"), 4).0, [0; 4]);

	// In the window's own background, whatever its cells', or Neovim's default one until known
	let background_of = |redraw: &Redraw| {
		let layer = redraw.layers.iter().find(|layer| layer.id == 4).unwrap();
		layer.padding_background
	};
	editor.set_float_style(win_of(4), FloatStyle { padding: Some(Edges::all(1.0)), ..FloatStyle::default() });
	assert_eq!(background_of(&editor.refresh().expect("padded")), (editor.background(), 0));
	let own =
		FloatStyle { padding: Some(Edges::all(1.0)), background: Some(0x123456), blend: 30, ..FloatStyle::default() };
	editor.set_float_style(win_of(4), own);
	assert_eq!(background_of(&editor.refresh().expect("its background")), (0x123456, 30));
}

#[test]
fn external_windows_are_laid_out_like_floats_with_a_border() {
	let mut editor = screen();
	// Window 2 at row 1 with a winbar, and a 2×1 external window with 4 pixels of padding, on the 80×80 pixel
	// screen
	let margins = WinViewportMarginsEvent { grid: Grid(2), win: IgnoredAny, top: 1, bottom: 0, left: 0, right: 0 };
	let events = vec![
		resize(2, 10, 3),
		win_pos(2, 1, 0),
		viewport(2, 9, 0),
		RedrawNotification::WinViewportMargins(vec![margins]),
		resize(4, 2, 1),
		external(4),
	];
	flush(&mut editor, events);
	editor.set_float_style(win_of(4), FloatStyle { padding: Some(Edges::all(4.0)), ..FloatStyle::default() });
	let at = |editor: &mut Editor, place| {
		editor.set_external_place(win_of(4), place);
		let redraw = editor.refresh().expect("placed");
		let layer = redraw.layers.iter().find(|layer| layer.id == 4).unwrap();
		(layer.x, layer.y)
	};
	let place = |relative, row, col, fixed| Place::Relative {
		relative,
		anchor: Anchor::NorthWest,
		row,
		col,
		offset: [0.0; 2],
		fixed,
	};

	// From window 2's text, below its winbar; its frame there, its cells inside it
	assert_eq!(at(&mut editor, place(Relative::Win(win_of(2)), 0.0, 0.0, false)), (4, 2 * 16 + 4));
	// Kept on the screen with its frame: 24×24 pixels at most at 56, 56
	assert_eq!(at(&mut editor, place(Relative::Editor, 4.0, 9.0, false)), (60, 60));
	// Above the command line
	editor.set_command_rows(1);
	assert_eq!(at(&mut editor, place(Relative::Editor, 4.0, 9.0, false)), (60, 44));
	// `fixed`: not moved sideways, so cut off on the right
	assert_eq!(at(&mut editor, place(Relative::Editor, 4.0, 9.0, true)), (9 * 8 + 4, 44));
}

#[test]
fn external_windows_placed_together_share_a_shadow() {
	let mut editor = screen();
	let shadow = Shadow { blur: 8.0, offset: [0.0, 0.0], color: 0, opacity: 0.5 };
	editor.set_default_float_style(FloatStyle { shadow: Some(shadow), ..FloatStyle::default() });
	// 2×1 external windows: 4 at the top left, 5 just below it, and 6 on its own, centered
	let mut events = vec![];
	for id in 4..=6 {
		events.extend([viewport(id, 10, 0), resize(id, 2, 1), external(id)]);
	}
	flush(&mut editor, events);
	let at = |relative, row| Place::Relative {
		relative,
		anchor: Anchor::NorthWest,
		row,
		col: 0.0,
		offset: [0.0; 2],
		fixed: false,
	};
	editor.set_external_place(win_of(4), at(Relative::Editor, 0.0));
	editor.set_external_place(win_of(5), at(Relative::Win(win_of(4)), 1.0));
	let redraw = editor.refresh().expect("placed");

	// One shadow under 4 and 5, drawn with 4, the first; 6 has its own.
	assert_eq!(shadow_of(&redraw, 4), Some(vec![[0, 0, 2, 1], [0, 1, 2, 1]]));
	assert_eq!(shadow_of(&redraw, 5), None);
	assert_eq!(shadow_of(&redraw, 6).map(|area| area.len()), Some(1));
}

#[test]
fn a_float_stays_in_its_cells() {
	let mut editor = screen();
	let outline = Some(OutlineStyle { width: 2.0, highlight: String::from("FloatBorder"), color: None });
	editor.set_default_float_style(FloatStyle { outline, padding: Some(Edges::all(2.0)), ..FloatStyle::default() });
	// Placed at row 2, column 2: drawn there whatever its style, with no frame to move it
	let redraw = flush(&mut editor, vec![resize(3, 2, 1), float(3, 2, 2, 1)]);
	let layer = redraw.layers.iter().find(|layer| layer.id == 3).unwrap();
	assert_eq!((layer.x, layer.y, layer.padding, layer.outline), (2 * 8, 2 * 16, Edges::default(), None));
	// The mouse finds its cells where Neovim placed them.
	assert_eq!(editor.grid_at(17.0, 33.0), Some(GridPosition { grid: Grid(3), row: 0, col: 0 }));
	assert_eq!(editor.grid_at(25.0, 33.0), Some(GridPosition { grid: Grid(3), row: 0, col: 1 }));
}

#[test]
fn the_shadow_is_cast_from_the_outline() {
	let mut editor = screen();
	let shadow = Shadow { blur: 8.0, offset: [0.0, 0.0], color: 0, opacity: 0.5 };
	let outline = Some(OutlineStyle { width: 2.0, highlight: String::from("FloatBorder"), color: None });
	editor.set_default_float_style(FloatStyle {
		shadow: Some(shadow),
		outline,
		corners: None,
		padding: None,
		parent: None,
		..FloatStyle::default()
	});
	flush(&mut editor, vec![resize(3, 2, 1), external(3)]);
	let place = Place::Relative {
		relative: Relative::Editor,
		anchor: Anchor::NorthWest,
		row: 1.0,
		col: 1.0,
		offset: [0.0; 2],
		fixed: false,
	};
	editor.set_external_place(win_of(3), place);
	let redraw = editor.refresh().expect("placed");

	// The external window's cells and 2 pixels around them, which the outline covers: it doesn't hide the shadow's
	// edge. The cells moved by those 2 pixels from where it's placed, so the outline starts there.
	let layer = redraw.layers.iter().find(|layer| layer.id == 3).unwrap();
	let area = layer.shadow.as_ref().map(|shadow| shadow.area.clone());
	assert_eq!(area, Some(vec![renderer::Rect::new(CELL.width, CELL.height, 2 * CELL.width + 4, CELL.height + 4)]));
}

#[test]
fn floating_windows_have_the_shadow() {
	let shadow = Shadow { blur: 8.0, offset: [0.0, 2.0], color: 0, opacity: 0.5 };
	let mut editor = screen();
	flush(&mut editor, vec![resize(3, 2, 1), win_pos(2, 0, 0), float(3, 1, 1, 1)]);

	// None until set
	let redraw = flush(&mut editor, vec![line(3, 0, "a")]);
	assert_eq!(shadow_of(&redraw, 3), None);

	editor.set_default_float_style(FloatStyle {
		shadow: Some(shadow),
		outline: None,
		corners: None,
		padding: None,
		parent: None,
		..FloatStyle::default()
	});
	let redraw = flush(&mut editor, vec![line(3, 0, "b")]);
	// The float has it, under its own cells; the window doesn't.
	assert_eq!((shadow_of(&redraw, 3), shadow_of(&redraw, 2)), (Some(vec![[1, 1, 2, 1]]), None));

	// A float left out, until it isn't: either is a change to draw.
	editor.set_float_style(
		win_of(3),
		FloatStyle { shadow: None, outline: None, corners: None, padding: None, parent: None, ..FloatStyle::default() },
	);
	assert_eq!(shadow_of(&flush(&mut editor, vec![]), 3), None);
	editor.set_float_style(
		win_of(3),
		FloatStyle {
			shadow: Some(shadow),
			outline: None,
			corners: None,
			padding: None,
			parent: None,
			..FloatStyle::default()
		},
	);
	assert!(shadow_of(&flush(&mut editor, vec![]), 3).is_some());

	// Changes on sarthi's side show without waiting for Neovim's next redraw, and only if there are any.
	assert_eq!(editor.refresh(), None);
	editor.set_float_style(
		win_of(3),
		FloatStyle { shadow: None, outline: None, corners: None, padding: None, parent: None, ..FloatStyle::default() },
	);
	let redraw = editor.refresh().expect("the shadow changed");
	assert_eq!(shadow_of(&redraw, 3), None);

	// Forgotten with the float's grid
	flush(&mut editor, vec![RedrawNotification::GridDestroy(vec![GridDestroyEvent { grid: Grid(3) }])]);
	assert!(editor.floats.is_empty());
}

#[test]
fn floats_at_one_level_share_a_shadow() {
	let mut editor = screen();
	let shadow = Shadow { blur: 8.0, offset: [0.0, 0.0], color: 0, opacity: 0.5 };
	editor.set_default_float_style(FloatStyle {
		shadow: Some(shadow),
		outline: None,
		corners: None,
		padding: None,
		parent: None,
		..FloatStyle::default()
	});
	// 3 and 4 side by side at 50, like a picker's windows; 5 at 52 attached to 3, like a scrollbar; 6 at 100, a hover
	// over them; 7 at 50, fully transparent
	let mut events: Vec<_> = (3..=7).map(|id| resize(id, 2, 1)).collect();
	events.extend([
		float_at_level(3, 1, 1, 1, 50),
		float_at_level(4, 1, 3, 2, 50),
		float_at_level(5, 1, 5, 3, 52),
		float_at_level(6, 0, 2, 4, 100),
		float_at_level(7, 3, 0, 5, 50),
	]);
	flush(&mut editor, events);
	editor.set_float_style(
		win_of(5),
		FloatStyle {
			shadow: Some(shadow),
			outline: None,
			corners: None,
			padding: None,
			parent: Some(win_of(3)),
			..FloatStyle::default()
		},
	);
	editor.set_float_style(
		win_of(7),
		FloatStyle { shadow: None, outline: None, corners: None, padding: None, parent: None, ..FloatStyle::default() },
	);
	let redraw = flush(&mut editor, vec![]);

	// One shadow for 3, 4 and the attached 5, drawn with the first of them
	assert_eq!(shadow_of(&redraw, 3), Some(vec![[1, 1, 2, 1], [3, 1, 2, 1], [5, 1, 2, 1]]));
	assert_eq!((shadow_of(&redraw, 4), shadow_of(&redraw, 5), shadow_of(&redraw, 7)), (None, None, None));
	// The hover has its own.
	assert_eq!(shadow_of(&redraw, 6), Some(vec![[2, 0, 2, 1]]));

	// 4 with a shadow of its own: one for it, drawn with it, over 3's
	let own = Shadow { opacity: 0.8, ..shadow };
	editor.set_float_style(
		win_of(4),
		FloatStyle {
			shadow: Some(own),
			outline: None,
			corners: None,
			padding: None,
			parent: None,
			..FloatStyle::default()
		},
	);
	let redraw = flush(&mut editor, vec![]);
	assert_eq!(shadow_of(&redraw, 3), Some(vec![[1, 1, 2, 1], [5, 1, 2, 1]]));
	assert_eq!(shadow_of(&redraw, 4), Some(vec![[3, 1, 2, 1]]));
}

/// The area of the shadow drawn with grid `id`'s layer in `redraw`, in cells of the screen
fn shadow_of(redraw: &Redraw, id: u64) -> Option<Vec<[u32; 4]>> {
	let layer = redraw.layers.iter().find(|layer| layer.id == id).unwrap();
	let cells =
		|[x, y, width, height]: [u32; 4]| [x / CELL.width, y / CELL.height, width / CELL.width, height / CELL.height];
	layer
		.shadow
		.as_ref()
		.map(|shadow| shadow.area.iter().map(|&rect| cells(rect.into())).collect())
}

#[test]
fn some_windows_jump_or_slide_their_own_way() {
	let mut editor = screen();
	let motion = Motion::Spring { stiffness: 100.0, damping: 20.0 };
	flush(&mut editor, vec![win_pos(2, 0, 0), viewport(2, 100, 0)]);

	// Told before the redraw that scrolls, like `WinScrolled` does
	editor.set_scroll_motion(win_of(2), None);
	let redraw = flush(&mut editor, vec![viewport(2, 100, 1), line(2, 0, "a")]);
	assert_eq!(scroll_of(&redraw, 2), None);

	editor.set_scroll_motion(win_of(2), Some(motion));
	let redraw = flush(&mut editor, vec![viewport(2, 100, 1), line(2, 0, "b")]);
	assert_eq!(scroll_of(&redraw, 2), Some((1, 0, 3, 0, 4)));
	let Some(layer) = redraw.layers.iter().find(|layer| layer.id == 2) else { panic!() };
	assert_eq!(layer.scroll.and_then(|scroll| scroll.motion), Some(motion));

	// Forgotten with the window's grid
	editor.set_scroll_motion(win_of(2), None);
	flush(&mut editor, vec![RedrawNotification::GridDestroy(vec![GridDestroyEvent { grid: Grid(2) }])]);
	assert!(editor.scroll_motions.is_empty());
}

fn option(name: &str, value: OptionValue) -> RedrawNotification {
	RedrawNotification::OptionSet(vec![OptionSetEvent { name: name.to_owned(), value }])
}

#[test]
fn guifont_changes_are_drawn() {
	let mut editor = screen();
	// Neovim's default comes first, at attach.
	let default = || option("guifont", OptionValue::String(String::from("Source Code Pro,monospace")));
	flush(&mut editor, vec![default()]);
	assert_eq!(editor.guifont(), "");
	assert_eq!(editor.strip_default_guifont("Source Code Pro,monospace"), "");
	assert_eq!(editor.strip_default_guifont("Iosevka"), "Iosevka");

	let guifont = || option("guifont", OptionValue::String(String::from("Iosevka:h14")));
	assert!(editor.apply(vec![guifont(), RedrawNotification::Flush]).is_some());
	assert_eq!(editor.guifont(), "Iosevka:h14");

	// The same value again, or another option, changes nothing to draw.
	assert_eq!(editor.apply(vec![guifont(), RedrawNotification::Flush]), None);
	assert_eq!(editor.apply(vec![option("ttimeoutlen", OptionValue::Number(50)), RedrawNotification::Flush]), None);

	// Set back to the default, it counts as unset again.
	assert!(editor.apply(vec![default(), RedrawNotification::Flush]).is_some());
	assert_eq!(editor.guifont(), "");
}

#[test]
fn linespace_changes_are_drawn() {
	let mut editor = screen();
	flush(&mut editor, vec![option("linespace", OptionValue::Number(0))]);
	assert_eq!(editor.linespace(), 0);

	let linespace = || option("linespace", OptionValue::Number(3));
	assert!(editor.apply(vec![linespace(), RedrawNotification::Flush]).is_some());
	assert_eq!(editor.linespace(), 3);
	assert_eq!(editor.apply(vec![linespace(), RedrawNotification::Flush]), None);
}

#[test]
fn mouse_on_and_off() {
	let mut editor = screen();
	assert!(!editor.mouse());
	editor.apply(vec![RedrawNotification::MouseOn]);
	assert!(editor.mouse());
	editor.apply(vec![RedrawNotification::MouseOff]);
	assert!(!editor.mouse());

	assert!(!editor.mouse_move_events());
	editor.apply(vec![option("mousemoveevent", OptionValue::Bool(true))]);
	assert!(editor.mouse_move_events());
}

#[test]
fn the_grid_under_a_cell() {
	// The 10×5 screen, window grid 2 (4×3) at row 0, col 6, and float 6 (3×2) over it at row 1, col 5
	let mut editor = screen();
	flush(&mut editor, vec![resize(6, 3, 2), win_pos(2, 0, 6), float(6, 1, 5, 1)]);
	let at = |editor: &Editor, row, col| {
		let (x, y) = middle(row, col);
		editor.grid_at(x, y)
	};
	let cell = |grid, row, col| Some(GridPosition { grid: Grid(grid), row, col });

	assert_eq!(at(&editor, 0, 7), cell(2, 0, 1));
	assert_eq!(at(&editor, 0, 2), cell(1, 0, 2));
	// The float covers the window.
	assert_eq!(at(&editor, 2, 6), cell(6, 1, 1));
	assert_eq!(at(&editor, 2, 9), cell(2, 2, 3));
	// Outside the screen
	assert_eq!(at(&editor, 5, 0), None);
	assert_eq!(at(&editor, 0, 10), None);

	// A float made with `mouse = false` lets clicks through to the window below.
	let mut no_mouse = float(6, 1, 5, 1);
	let RedrawNotification::WinFloatPos(positions) = &mut no_mouse else { unreachable!() };
	positions[0].mouse_enabled = false;
	flush(&mut editor, vec![no_mouse]);
	assert_eq!(at(&editor, 2, 6), cell(2, 2, 0));

	assert_eq!(
		(editor.grid_origin(Grid(1)), editor.grid_origin(Grid(2)), editor.grid_origin(Grid(6))),
		(Some((0, 0)), Some((0, 6)), Some((1, 5)))
	);
	assert_eq!(editor.grid_origin(Grid(9)), None);
}

#[test]
fn the_message_grid_covers_its_rows_only() {
	let mut editor = screen();
	flush(&mut editor, vec![resize(3, 10, 5), messages(3, false, 2)]);
	let at = |row, col| {
		let (x, y) = middle(row, col);
		editor.grid_at(x, y)
	};
	assert_eq!(at(3, 4), Some(GridPosition { grid: Grid(3), row: 0, col: 4 }));
	assert_eq!(at(2, 4), Some(GridPosition { grid: Grid(1), row: 2, col: 4 }));
	assert_eq!(editor.grid_origin(Grid(3)), Some((3, 0)));
}
