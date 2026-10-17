//! Dependency layering rules (docs/architecture.md).
//!
//! Each workspace crate has a layer; a crate may depend only on lower layers, and only the UI
//! layer may use a UI toolkit. Binaries and tooling are exempt.

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Layer(u8),
    /// Layer 0 that depends on no workspace crate at all.
    Standalone,
    Exempt,
}

/// The layering table. Names are package names without the `gitcraft-` prefix.
pub const TABLE: &[(&str, Class)] = &[
    ("control-key", Class::Standalone),
    ("git", Class::Layer(1)),
    ("graph", Class::Layer(2)),
    ("engine", Class::Layer(4)),
    ("mcp", Class::Layer(5)),
    ("ui-egui", Class::Layer(6)),
    ("gitcraft", Class::Exempt),
    ("cli", Class::Exempt),
    ("xtask", Class::Exempt),
];

/// External crates that are a UI toolkit or windowing dependency.
pub const UI_CRATES: &[&str] = &["egui", "eframe", "winit", "egui_kittest", "rfd", "egui_extras"];
pub const UI_MIN_LAYER: u8 = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dep {
    pub name: String,
    pub workspace: bool,
    pub dev: bool,
}

#[derive(Debug, Clone)]
pub struct Crate {
    pub name: String,
    pub deps: Vec<Dep>,
}

pub fn short_name(name: &str) -> &str {
    name.strip_prefix("gitcraft-").unwrap_or(name)
}

pub fn classify(name: &str) -> Option<Class> {
    let s = short_name(name);
    TABLE.iter().find(|(n, _)| *n == s).map(|(_, c)| *c)
}

pub fn describe(c: Option<Class>) -> String {
    match c {
        Some(Class::Layer(l)) => format!("L{l}"),
        Some(Class::Standalone) => "L0 (alone)".into(),
        Some(Class::Exempt) => "app/tool".into(),
        None => "unknown".into(),
    }
}

fn layer(c: Class) -> Option<u8> {
    match c {
        Class::Layer(l) => Some(l),
        Class::Standalone => Some(0),
        Class::Exempt => None,
    }
}

pub fn from_metadata(m: &Value) -> Result<Vec<Crate>, String> {
    let pkgs = m.get("packages").and_then(Value::as_array).ok_or("cargo metadata: no packages")?;
    let names: Vec<&str> = pkgs.iter().filter_map(|p| p.get("name").and_then(Value::as_str)).collect();
    Ok(pkgs
        .iter()
        .filter_map(|p| {
            let name = p.get("name")?.as_str()?.to_string();
            let deps = p
                .get("dependencies")?
                .as_array()?
                .iter()
                .filter_map(|d| {
                    let n = d.get("name")?.as_str()?.to_string();
                    let dev = d.get("kind").and_then(Value::as_str) == Some("dev");
                    Some(Dep { workspace: names.contains(&n.as_str()), name: n, dev })
                })
                .collect();
            Some(Crate { name, deps })
        })
        .collect())
}

pub fn check(crates: &[Crate]) -> Vec<String> {
    let mut v = Vec::new();
    for c in crates {
        let Some(class) = classify(&c.name) else {
            v.push(format!("{} is not in the layering table (xtask/src/layers.rs)", c.name));
            continue;
        };
        let Some(mine) = layer(class) else { continue };
        for d in c.deps.iter().filter(|d| !d.dev) {
            if d.workspace {
                if class == Class::Standalone {
                    v.push(format!("{} must not depend on workspace crates (uses {})", c.name, d.name));
                }
                match classify(&d.name).and_then(layer) {
                    Some(l) if l < mine => {}
                    Some(l) => v.push(format!("{} (L{mine}) depends on {} (L{l}): only lower layers", c.name, d.name)),
                    None => v.push(format!("{} depends on the app/tool crate {}", c.name, d.name)),
                }
            } else if UI_CRATES.contains(&d.name.as_str()) && mine < UI_MIN_LAYER {
                v.push(format!("{} (L{mine}) uses the UI crate {}: only L{UI_MIN_LAYER} may", c.name, d.name));
            }
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn krate(name: &str, deps: &[(&str, bool)]) -> Crate {
        Crate { name: name.into(), deps: deps.iter().map(|(n, ws)| Dep { name: (*n).into(), workspace: *ws, dev: false }).collect() }
    }

    #[test]
    fn rules() {
        assert!(check(&[krate("gitcraft-engine", &[("gitcraft-git", true), ("gitcraft-graph", true)])]).is_empty());
        assert_eq!(check(&[krate("gitcraft-git", &[("gitcraft-engine", true)])]).len(), 1);
        assert_eq!(check(&[krate("gitcraft-engine", &[("egui", false)])]).len(), 1);
        assert!(check(&[krate("gitcraft-ui-egui", &[("egui", false), ("gitcraft-engine", true)])]).is_empty());
        assert_eq!(check(&[krate("gitcraft-control-key", &[("gitcraft-git", true)])]).len(), 2);
        assert_eq!(check(&[krate("gitcraft-new", &[])]).len(), 1);
    }
}
