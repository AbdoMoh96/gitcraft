//! The commit graph as lines of text: lanes drawn with box-drawing characters, then the short id,
//! the refs pointing at the commit and its summary.

use gitcraft_engine::RepoData;
use gitcraft_engine::graph::Seg;

/// Lanes drawn at most (wider graphs are cut, marked with `…`).
const MAX_DRAWN: usize = 40;

fn grid(width: usize) -> Vec<char> {
    vec![' '; width.min(MAX_DRAWN) * 2]
}

fn put(g: &mut [char], lane: usize, c: char) {
    if let Some(slot) = g.get_mut(lane * 2) {
        *slot = c;
    }
}

fn fill(g: &mut [char], a: usize, b: usize) {
    let (lo, hi) = (a.min(b), a.max(b));
    for col in lo * 2 + 1..hi * 2 {
        if let Some(slot) = g.get_mut(col)
            && *slot == ' '
        {
            *slot = '─';
        }
    }
}

/// The joining line between two rows, when any segment runs sideways. `top`: segments ending at
/// the row's commit (drawn above it); otherwise segments leaving it (drawn below).
fn connector(segs: &[Seg], width: usize, lane: usize, top: bool) -> Option<String> {
    if segs.iter().all(|s| s.from == s.to) {
        return None;
    }
    let mut g = grid(width);
    for s in segs.iter().filter(|s| s.from == s.to) {
        put(&mut g, s.from, '│');
    }
    let straight = |i: usize| segs.iter().any(|s| s.from == i && s.to == i);
    for s in segs.iter().filter(|s| s.from != s.to) {
        let other = if top { s.from } else { s.to };
        fill(&mut g, lane, other);
        let right = other > lane;
        put(
            &mut g,
            other,
            match (top, right) {
                (true, true) => '╯',
                (true, false) => '╰',
                (false, true) => '╮',
                (false, false) => '╭',
            },
        );
        let at_lane = if straight(lane) || !top {
            if right { '├' } else { '┤' }
        } else if right {
            '╰'
        } else {
            '╯'
        };
        put(&mut g, lane, at_lane);
    }
    Some(g.into_iter().collect::<String>().trim_end().to_string())
}

pub fn render(data: &RepoData) -> Vec<String> {
    let width = data.graph.width.max(1);
    let mut out = Vec::new();
    for (row, c) in data.graph.rows.iter().zip(&data.commits) {
        if let Some(l) = connector(&row.up, width, row.lane, true) {
            out.push(l);
        }
        let mut g = grid(width);
        for s in row.up.iter().chain(&row.down).filter(|s| s.from == s.to) {
            put(&mut g, s.from, '│');
        }
        put(&mut g, row.lane, if c.is_merge() { '◎' } else { '●' });
        if width > MAX_DRAWN
            && let Some(last) = g.last_mut()
        {
            *last = '…';
        }
        let labels: Vec<String> = data
            .labels
            .get(&c.oid)
            .map(|ix| {
                ix.iter().filter_map(|&i| data.refs.get(i)).map(|r| if r.head { format!("HEAD → {}", r.name) } else { r.name.clone() }).collect()
            })
            .unwrap_or_default();
        let deco = if labels.is_empty() { String::new() } else { format!(" ({})", labels.join(", ")) };
        out.push(format!("{} {}{deco} {}", g.into_iter().collect::<String>(), c.short(), c.summary));
        if let Some(l) = connector(&row.down, width, row.lane, false) {
            out.push(l);
        }
    }
    if data.more {
        out.push("… more commits (--limit N)".into());
    }
    out
}
