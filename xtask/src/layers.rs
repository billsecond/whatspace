//! Dependency layering rules (docs/architecture.md).
//!
//! The rule engine works on a small, metadata-independent model so it can be
//! unit-tested; `from_metadata` builds that model from `cargo metadata`.

use serde_json::Value;

/// Where a workspace crate sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Regular layered crate.
    Layer(u8),
    /// Binaries and build tooling: exempt from the rules.
    Exempt,
}

impl Class {
    fn layer(self) -> Option<u8> {
        match self {
            Class::Layer(l) => Some(l),
            Class::Exempt => None,
        }
    }
}

/// The layering table. Names are package names without the `whatspace-` prefix.
pub const TABLE: &[(&str, Class)] = &[
    // L0: the platform (what each system and file system says) and the model (pure data).
    ("fs", Class::Layer(0)),
    ("model", Class::Layer(0)),
    // L1: the scanner, which puts the two together.
    ("scan", Class::Layer(1)),
    // L2: the front end.
    ("ui-egui", Class::Layer(2)),
    // apps and tooling
    ("whatspace", Class::Exempt),
    ("cli", Class::Exempt),
    ("xtask", Class::Exempt),
];

/// External crates that constitute a UI toolkit / windowing dependency.
/// Entries ending in `*` are prefixes.
pub const UI_CRATES: &[&str] = &["egui", "eframe", "winit", "egui_kittest", "rfd", "bevy*"];

/// First layer allowed to use UI crates.
pub const UI_MIN_LAYER: u8 = 2;

pub fn short_name(pkg: &str) -> &str {
    pkg.strip_prefix("whatspace-").unwrap_or(pkg)
}

pub fn classify(pkg: &str) -> Option<Class> {
    let s = short_name(pkg);
    TABLE.iter().find(|(n, _)| *n == s).map(|(_, c)| *c)
}

fn is_ui_crate(name: &str) -> bool {
    UI_CRATES.iter().any(|p| match p.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == *p,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepKind {
    Normal,
    Dev,
    Build,
}

#[derive(Debug, Clone)]
pub struct Dep {
    pub name: String,
    pub kind: DepKind,
    /// `true` if the dependency is a workspace member.
    pub workspace: bool,
}

#[derive(Debug, Clone)]
pub struct Crate {
    pub name: String,
    pub deps: Vec<Dep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    Unregistered { krate: String },
    Upward { krate: String, dep: String, from: u8, to: u8, kind: DepKind },
    UiTooLow { krate: String, dep: String, layer: u8 },
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Violation::Unregistered { krate } => {
                write!(f, "{krate}: unknown workspace crate; register it in xtask/src/layers.rs TABLE (see docs/architecture.md)")
            }
            Violation::Upward { krate, dep, from, to, kind } => {
                write!(f, "{krate} (L{from}) -> {dep} (L{to}) [{kind:?}]: may only depend on strictly lower layers")
            }
            Violation::UiTooLow { krate, dep, layer } => {
                write!(f, "{krate} (L{layer}) depends on UI crate `{dep}`; UI toolkits are only allowed in L{UI_MIN_LAYER}+")
            }
        }
    }
}

/// Check all rules. Returns violations sorted for stable output.
pub fn check(crates: &[Crate]) -> Vec<Violation> {
    let mut out = Vec::new();
    for c in crates {
        let Some(class) = classify(&c.name) else {
            out.push(Violation::Unregistered { krate: c.name.clone() });
            continue;
        };
        let Some(layer) = class.layer() else { continue };
        for d in &c.deps {
            // Self dev-dependencies (e.g. to enable features in tests) are fine.
            if d.name == c.name {
                continue;
            }
            if d.workspace {
                if let Some(dc) = classify(&d.name) {
                    let to = dc.layer().unwrap_or(u8::MAX);
                    if to >= layer {
                        out.push(Violation::Upward {
                            krate: c.name.clone(),
                            dep: d.name.clone(),
                            from: layer,
                            to: if to == u8::MAX { 9 } else { to },
                            kind: d.kind,
                        });
                    }
                }
            } else if layer < UI_MIN_LAYER && is_ui_crate(&d.name) {
                out.push(Violation::UiTooLow { krate: c.name.clone(), dep: d.name.clone(), layer });
            }
        }
    }
    out.sort_by_key(|v| v.to_string());
    out.dedup();
    out
}

/// Build the model from `cargo metadata --format-version 1 --no-deps`.
pub fn from_metadata(meta: &Value) -> Result<Vec<Crate>, String> {
    let pkgs = meta["packages"].as_array().ok_or("metadata: no packages array")?;
    let members: Vec<&str> = pkgs.iter().filter_map(|p| p["name"].as_str()).collect();
    let mut out = Vec::new();
    for p in pkgs {
        let name = p["name"].as_str().ok_or("package without name")?.to_owned();
        let mut deps = Vec::new();
        for d in p["dependencies"].as_array().into_iter().flatten() {
            let dname = d["name"].as_str().unwrap_or_default().to_owned();
            let kind = match d["kind"].as_str() {
                Some("dev") => DepKind::Dev,
                Some("build") => DepKind::Build,
                _ => DepKind::Normal,
            };
            let workspace = members.contains(&dname.as_str()) || d["path"].is_string();
            deps.push(Dep { name: dname, kind, workspace });
        }
        out.push(Crate { name, deps });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub fn describe(class: Option<Class>) -> String {
    match class {
        Some(Class::Layer(l)) => format!("L{l}"),
        Some(Class::Exempt) => "exempt".into(),
        None => "UNREGISTERED".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(name: &str, deps: &[(&str, DepKind, bool)]) -> Crate {
        Crate { name: name.into(), deps: deps.iter().map(|(n, k, w)| Dep { name: (*n).into(), kind: *k, workspace: *w }).collect() }
    }
    use DepKind::*;

    #[test]
    fn clean_downward_graph_passes() {
        let g = [
            c("whatspace-fs", &[("sysinfo", Normal, false)]),
            c("whatspace-model", &[("serde", Normal, false)]),
            c("whatspace-scan", &[("whatspace-fs", Normal, true), ("whatspace-model", Normal, true)]),
            c("whatspace-ui-egui", &[("whatspace-scan", Normal, true), ("egui", Normal, false)]),
            c("whatspace-cli", &[("whatspace-scan", Normal, true)]),
        ];
        assert!(check(&g).is_empty(), "{:?}", check(&g));
    }

    #[test]
    fn upward_and_sideways_dependencies_flagged() {
        let v = check(&[c("whatspace-model", &[("whatspace-scan", Normal, true)])]);
        assert!(matches!(v[..], [Violation::Upward { from: 0, to: 1, .. }]));
        let v = check(&[c("whatspace-model", &[("whatspace-fs", Normal, true)])]);
        assert!(matches!(v[..], [Violation::Upward { from: 0, to: 0, .. }]), "the two L0 crates stay apart");
        let v = check(&[c("whatspace-fs", &[("whatspace-model", Dev, true)])]);
        assert!(matches!(v[..], [Violation::Upward { kind: Dev, .. }]));
    }

    #[test]
    fn ui_crates_below_the_front_end_flagged() {
        for dep in ["egui", "eframe", "winit", "egui_kittest", "rfd"] {
            let v = check(&[c("whatspace-scan", &[(dep, Normal, false)])]);
            assert!(matches!(v[..], [Violation::UiTooLow { layer: 1, .. }]), "{dep}");
        }
        assert!(check(&[c("whatspace-ui-egui", &[("egui", Normal, false)])]).is_empty());
    }

    #[test]
    fn unregistered_crate_is_error() {
        let v = check(&[c("whatspace-mystery", &[])]);
        assert!(matches!(&v[..], [Violation::Unregistered { krate }] if krate == "whatspace-mystery"));
        assert!(v[0].to_string().contains("register"));
    }

    #[test]
    fn apps_and_xtask_exempt() {
        for app in ["whatspace", "whatspace-cli", "xtask"] {
            assert!(check(&[c(app, &[("egui", Normal, false), ("whatspace-ui-egui", Normal, true)])]).is_empty());
        }
    }

    #[test]
    fn metadata_parsing() {
        let meta: Value = serde_json::from_str(
            r#"{"packages":[
                {"name":"whatspace-scan","dependencies":[
                    {"name":"whatspace-fs","kind":null,"path":"/x/crates/fs"},
                    {"name":"thiserror","kind":null},
                    {"name":"proptest","kind":"dev"}]},
                {"name":"whatspace-fs","dependencies":[]}
            ]}"#,
        )
        .unwrap();
        let g = from_metadata(&meta).unwrap();
        assert_eq!(g.len(), 2);
        let scan = g.iter().find(|c| c.name == "whatspace-scan").unwrap();
        assert!(scan.deps[0].workspace && !scan.deps[1].workspace);
        assert_eq!(scan.deps[2].kind, Dev);
        assert!(check(&g).is_empty());
    }
}
