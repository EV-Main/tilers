//! JSON (de)serialization of the packed layout. [`PackedGrid`] already derives serde, so a
//! layout round-trips to JSON directly — `PackedGrid` *is* the serialized value. This module
//! is the seam for keeping that JSON **stable across versions** (schema tag + migrations);
//! `load` errors on a younger payload rather than silently wiping the user's workspace.

use crate::model::packed::PackedGrid;

/// Bump when the on-disk shape changes; `load` would migrate older payloads. v2: cell floors are
/// derived, so v1's stored `min_w`/`min_h` are ignored on load. v4: groups may float above
/// the grid, which a v3 reader would drop. v5: a cell's `w` is its width, where v3 and v4 may hold it in `collapsed`.
pub const SCHEMA_VERSION: u32 = 5;

/// Versioned wrapper actually written to storage.
#[derive(serde::Deserialize, serde::Serialize)]
pub struct Serialized {
	pub version: u32,
	pub grid: PackedGrid,
}

pub fn save(grid: &PackedGrid) -> String {
	serde_json::to_string(&Serialized {
		version: SCHEMA_VERSION,
		grid: grid.clone(),
	})
	.expect("PackedGrid is always serializable")
}

/// Parse a saved layout. Errors (not a silent default) on malformed/younger JSON so a corrupt
/// or future-version layout surfaces instead of silently resetting the workspace.
pub fn load(json: &str) -> Result<PackedGrid, LoadError> {
	let serialized: Serialized = serde_json::from_str(json).map_err(LoadError::Parse)?;
	if serialized.version > SCHEMA_VERSION {
		return Err(LoadError::FutureVersion(serialized.version));
	}
	let mut grid = serialized.grid;
	if serialized.version < 5 {
		widen(&mut grid, json)?;
	}
	Ok(grid)
}

#[derive(Debug)]
pub enum LoadError {
	Parse(serde_json::Error),
	/// Payload is newer than this build understands.
	FutureVersion(u32),
}

/// Give each cell the width `collapsed` holds for it, what lies right of it moving over.
fn widen(grid: &mut PackedGrid, json: &str) -> Result<(), LoadError> {
	#[derive(serde::Deserialize)]
	struct Payload {
		grid: Grid,
	}
	#[derive(serde::Deserialize)]
	struct Grid {
		cells: Vec<Held>,
	}
	#[derive(serde::Deserialize)]
	struct Held {
		collapsed: Option<u32>,
	}
	let payload: Payload = serde_json::from_str(json).map_err(LoadError::Parse)?;
	for (i, held) in payload.grid.cells.into_iter().enumerate() {
		let Some(w) = held.collapsed else { continue };
		let c = &mut grid.cells[i];
		let (edge, by) = (c.x + c.w, w.saturating_sub(c.w));
		c.w += by;
		c.desired_w = None;
		for o in grid.cells.iter_mut().filter(|o| o.x >= edge) {
			o.x += by;
			o.desired_x = None;
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::model::{Group, GroupId, PanelId};

	/// A non-trivial grid driven through the real API so the round-trip proves `next_group_id`
	/// (private, but observable via id collisions) and cell geometry survive.
	fn sample() -> PackedGrid {
		let mut g = PackedGrid::default();
		let gid = g.mint_group_id();
		g.place(Group::new(gid, PanelId("a".into())), 3, 2, (1, 1), 6);
		let gid = g.mint_group_id();
		g.place(Group::new(gid, PanelId("b".into())), 2, 4, (1, 1), 6);
		g.add_tab(GroupId(0), PanelId("c".into()));
		let gid = g.mint_group_id();
		g.place(Group::new(gid, PanelId("d".into())), 2, 2, (1, 1), 6);
		g.lift(gid, 1, 1);
		g
	}

	/// Floors are derived from the host's declared mins, so a round-trip drops them by design.
	fn layout(mut g: PackedGrid) -> PackedGrid {
		for c in &mut g.cells {
			(c.min_w, c.min_h) = (0, 0);
		}
		for f in &mut g.floats {
			(f.min_w, f.min_h) = (0, 0);
		}
		g
	}

	#[test]
	fn round_trips() {
		let g = sample();
		assert_eq!(load(&save(&g)).expect("loads"), layout(g), "round-trip must preserve the layout (incl. next_group_id)");
	}

	#[test]
	fn v1_loads_without_its_stored_floors() {
		let v1 = r#"{"version":1,"grid":{"cells":[{"group":{"id":0,"tabs":["a"],"active":0},"x":0,"y":0,"w":3,"h":2,"min_w":2,"min_h":2,"desired_w":null,"desired_x":null,"rank":0}],"state":"Settled","next_group_id":1}}"#;
		let g = load(v1).expect("a v1 payload loads");
		let c = &g.cells[0];
		assert_eq!((c.x, c.y, c.w, c.h, c.min_w, c.min_h, c.desired_h), (0, 0, 3, 2, 0, 0, None));
	}

	#[test]
	fn v4_opens_its_collapsed_cells() {
		let cell =
			|id: u64, x: u32, w: u32, collapsed: &str| format!(r#"{{"group":{{"id":{id},"tabs":["p{id}"],"active":0}},"x":{x},"y":0,"w":{w},"h":24,"rank":{id},"collapsed":{collapsed}}}"#);
		let cells = [cell(0, 0, 8, "null"), cell(1, 8, 1, "8"), cell(2, 9, 8, "null"), cell(3, 17, 1, "8")].join(",");
		let v4 = format!(r#"{{"version":4,"grid":{{"cells":[{cells}],"state":"Settled","next_group_id":4}}}}"#);
		let g = load(&v4).expect("a v4 payload loads");
		assert_eq!(g.cells.iter().map(|c| (c.x, c.w)).collect::<Vec<_>>(), [(0, 8), (8, 8), (16, 8), (24, 8)]);
		g.assert_packed();
	}

	#[test]
	fn future_version_errors_not_resets() {
		let json = save(&sample()).replace(&format!("\"version\":{SCHEMA_VERSION}"), &format!("\"version\":{}", SCHEMA_VERSION + 1));
		assert!(matches!(load(&json), Err(LoadError::FutureVersion(v)) if v == SCHEMA_VERSION + 1));
	}

	#[test]
	fn garbage_errors() {
		assert!(matches!(load("not json"), Err(LoadError::Parse(_))));
	}
}
