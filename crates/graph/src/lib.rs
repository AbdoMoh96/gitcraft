//! The commit graph: which lane (column) each commit sits in, and the line segments drawn in each
//! row to join commits to their parents.
//!
//! Commits arrive newest first with every child before its parents (`git log --date-order`).
//! A lane holds the commit it is waiting for; a commit takes the leftmost lane waiting for it (or
//! a free one), the other lanes waiting for it merge into it, its first parent continues its
//! lane (unless a lane to its left already waits for that parent) and each further parent gets
//! the lane already waiting for it or a new one. Lanes never
//! move sideways, so a lane passing a row is a straight vertical line.
//!
//! Each row is drawn in two halves: [`Row::up`] runs from the top edge to the middle (where the
//! commit's dot is) and [`Row::down`] from the middle to the bottom edge.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use serde::Serialize;

/// The most lanes laid out; commits beyond share the last lane (a huge octopus or thousands of
/// parallel branches stay drawable).
pub const MAX_LANES: usize = 256;

/// A line in one half of a row, from lane `from` to lane `to`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Seg {
    pub from: usize,
    pub to: usize,
    /// Index into the palette (stable along a lane).
    pub color: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Row {
    /// The commit's lane.
    pub lane: usize,
    pub color: usize,
    /// Top edge → middle: lanes passing through, and lanes ending at this commit.
    pub up: Vec<Seg>,
    /// Middle → bottom edge: lanes passing through, and the lines to this commit's parents.
    pub down: Vec<Seg>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Layout {
    pub rows: Vec<Row>,
    /// The most lanes any row uses.
    pub width: usize,
}

struct Lane {
    waiting_for: String,
    color: usize,
}

/// Lay out commits given as `(id, parent ids)`, newest first.
pub fn layout<'a, I>(commits: I) -> Layout
where
    I: IntoIterator<Item = (&'a str, &'a [String])>,
{
    let mut lanes: Vec<Option<Lane>> = Vec::new();
    let mut next_color = 0usize;
    let mut out = Layout::default();
    let mut new_color = || {
        let c = next_color;
        next_color = next_color.wrapping_add(1);
        c
    };
    for (oid, parents) in commits {
        let waiting: Vec<usize> =
            lanes.iter().enumerate().filter(|(_, l)| l.as_ref().is_some_and(|l| l.waiting_for == oid)).map(|(i, _)| i).collect();
        let (lane, color) = match waiting.first() {
            Some(&i) => (i, lanes.get(i).and_then(|l| l.as_ref()).map(|l| l.color).unwrap_or(0)),
            None => (free_lane(&lanes), new_color()),
        };
        let mut row = Row { lane, color, up: Vec::new(), down: Vec::new() };
        for (i, l) in lanes.iter().enumerate() {
            if let Some(l) = l {
                let to = if l.waiting_for == oid { lane } else { i };
                row.up.push(Seg { from: i, to, color: l.color });
            }
        }
        for &i in &waiting {
            if let Some(slot) = lanes.get_mut(i) {
                *slot = None;
            }
        }
        let passing: Vec<usize> = lanes.iter().enumerate().filter(|(_, l)| l.is_some()).map(|(i, _)| i).collect();
        for (k, p) in parents.iter().enumerate() {
            let mut existing = lanes.iter().position(|l| l.as_ref().is_some_and(|l| &l.waiting_for == p));
            // The first parent continues this lane (both lines meet at the parent) unless a lane
            // to the left already waits for it: the main line stays straight.
            if k == 0 && existing.is_some_and(|j| j > lane) {
                existing = None;
            }
            match existing {
                Some(j) => {
                    let c = lanes.get(j).and_then(|l| l.as_ref()).map(|l| l.color).unwrap_or(color);
                    row.down.push(Seg { from: lane, to: j, color: c });
                }
                None => {
                    let (target, c) =
                        if k == 0 && lanes.get(lane).is_none_or(|l| l.is_none()) { (lane, color) } else { (free_lane(&lanes), new_color()) };
                    set_lane(&mut lanes, target, Lane { waiting_for: p.clone(), color: c });
                    row.down.push(Seg { from: lane, to: target, color: c });
                }
            }
        }
        for i in passing {
            if let Some(Some(l)) = lanes.get(i) {
                row.down.push(Seg { from: i, to: i, color: l.color });
            }
        }
        while lanes.last().is_some_and(|l| l.is_none()) {
            lanes.pop();
        }
        let used = row.up.iter().chain(&row.down).map(|s| s.from.max(s.to) + 1).max().unwrap_or(0).max(lane + 1);
        out.width = out.width.max(used);
        out.rows.push(row);
    }
    out
}

/// The leftmost free lane (capped at [`MAX_LANES`]).
fn free_lane(lanes: &[Option<Lane>]) -> usize {
    lanes.iter().position(Option::is_none).unwrap_or(lanes.len()).min(MAX_LANES - 1)
}

fn set_lane(lanes: &mut Vec<Option<Lane>>, i: usize, l: Lane) {
    while lanes.len() <= i {
        lanes.push(None);
    }
    if let Some(slot) = lanes.get_mut(i) {
        *slot = Some(l);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(spec: &[(&str, &[&str])]) -> Layout {
        let owned: Vec<(String, Vec<String>)> = spec.iter().map(|(o, ps)| (o.to_string(), ps.iter().map(|p| p.to_string()).collect())).collect();
        layout(owned.iter().map(|(o, ps)| (o.as_str(), ps.as_slice())))
    }

    #[test]
    fn straight_line() {
        let l = run(&[("c", &["b"]), ("b", &["a"]), ("a", &[])]);
        assert_eq!(l.width, 1);
        assert!(l.rows.iter().all(|r| r.lane == 0));
        assert_eq!(l.rows[0].up, vec![]);
        assert_eq!(l.rows[0].down, vec![Seg { from: 0, to: 0, color: 0 }]);
        assert_eq!(l.rows[2].down, vec![]);
    }

    #[test]
    fn branch_and_merge() {
        // m merges f into the main line; f and b both come from a.
        let l = run(&[("m", &["b", "f"]), ("f", &["a"]), ("b", &["a"]), ("a", &[])]);
        assert_eq!(l.width, 2);
        assert_eq!(l.rows[0].lane, 0);
        assert_eq!(l.rows[0].down.len(), 2);
        assert_eq!(l.rows[1].lane, 1);
        assert_eq!(l.rows[2].lane, 0);
        // At a, the lane from f joins lane 0.
        assert_eq!(l.rows[3].lane, 0);
        assert!(l.rows[3].up.iter().any(|s| s.from == 1 && s.to == 0));
        // The second lane keeps its own colour.
        assert_ne!(l.rows[1].color, l.rows[0].color);
    }

    #[test]
    fn two_heads_and_missing_parents() {
        // Two unrelated tips; x's parent is beyond the loaded history.
        let l = run(&[("x", &["gone"]), ("y", &["z"]), ("z", &[])]);
        assert_eq!(l.rows[1].lane, 1);
        // x's lane passes z's row on its way down.
        assert!(l.rows[2].down.iter().any(|s| s.from == 0 && s.to == 0));
    }

    #[test]
    fn freed_lanes_are_reused() {
        let l = run(&[("a2", &["a1"]), ("b2", &["b1"]), ("b1", &[]), ("c2", &["c1"]), ("c1", &[]), ("a1", &[])]);
        assert_eq!(l.rows[3].lane, 1);
        assert_eq!(l.width, 2);
    }

    #[test]
    fn many_parents_stay_bounded() {
        let ps: Vec<String> = (0..1000).map(|i| format!("p{i}")).collect();
        let l = layout([("octo", ps.as_slice())]);
        assert!(l.width <= MAX_LANES);
    }
}
