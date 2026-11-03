//! `layout.toml`: the tabs and panes to bring back on the next start (INTERFACES.md section 9).
//! It records what each pane was running. It never reconnects anything by itself (P6).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::{set_aside, write_atomic};
use crate::error::{AppError, ErrorCode};
use crate::terminal::SessionSpec;

const MAX_TABS: usize = 64;
const MAX_PANES: usize = 128;
const MAX_DEPTH: usize = 24;
const MAX_TITLE: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PaneNode {
    Leaf {
        pane_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spec: Option<SessionSpec>,
        title: String,
    },
    Split {
        direction: Direction,
        sizes: Vec<f64>,
        children: Vec<PaneNode>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Row,
    Column,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabLayout {
    pub id: String,
    pub title: String,
    /// The pane that had the keyboard.
    pub active: String,
    pub root: PaneNode,
    /// `files`, `docker` or `agent` when the tab shows a panel instead of terminals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub version: u32,
    pub active_tab: String,
    pub tabs: Vec<TabLayout>,
    /// The panes the window is divided into. Each leaf is a pane, shown in `group_tabs`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<PaneNode>,
    /// The tab each pane shows, by pane ID.
    pub group_tabs: std::collections::HashMap<String, String>,
    pub active_group: String,
}

fn invalid() -> AppError {
    AppError::new(ErrorCode::InvalidInput, "That layout isn't valid.")
}

fn check_node(node: &PaneNode, depth: usize, panes: &mut usize) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    match node {
        PaneNode::Leaf { pane_id, title, .. } => {
            *panes += 1;
            !pane_id.is_empty() && title.len() <= MAX_TITLE
        }
        PaneNode::Split {
            sizes, children, ..
        } => {
            let sum: f64 = sizes.iter().sum();
            children.len() >= 2
                && sizes.len() == children.len()
                && sizes.iter().all(|s| s.is_finite() && *s > 0.0)
                && (sum - 1.0).abs() < 1e-6
                && children.iter().all(|c| check_node(c, depth + 1, panes))
        }
    }
}

fn contains_pane(node: &PaneNode, id: &str) -> bool {
    match node {
        PaneNode::Leaf { pane_id, .. } => pane_id == id,
        PaneNode::Split { children, .. } => children.iter().any(|c| contains_pane(c, id)),
    }
}

impl Layout {
    /// Checks a layout from the interface or from a file that may have been edited.
    pub fn validate(&self) -> Result<(), AppError> {
        if self.tabs.len() > MAX_TABS {
            return Err(invalid());
        }
        let mut panes = 0;
        for tab in &self.tabs {
            if tab.title.len() > MAX_TITLE
                || !check_node(&tab.root, 0, &mut panes)
                || !contains_pane(&tab.root, &tab.active)
            {
                return Err(invalid());
            }
        }
        if panes > MAX_PANES {
            return Err(invalid());
        }
        if let Some(groups) = &self.groups {
            let mut count = 0;
            if !check_node(groups, 0, &mut count) || count > MAX_PANES {
                return Err(invalid());
            }
            if !self.group_tabs.keys().all(|g| contains_pane(groups, g))
                || !self
                    .group_tabs
                    .values()
                    .all(|t| self.tabs.iter().any(|x| &x.id == t))
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

/// Reads the saved layout. `None` means start fresh: there is no file, or it couldn't be used
/// (in which case it is moved aside to `layout.toml.bad`).
pub fn load(path: &Path) -> Option<Layout> {
    let text = std::fs::read_to_string(path).ok()?;
    match toml::from_str::<Layout>(&text) {
        // No tabs saved (every tab was closed) just means start fresh.
        Ok(layout) if layout.validate().is_ok() => Some(layout).filter(|l| !l.tabs.is_empty()),
        _ => {
            set_aside(path);
            None
        }
    }
}

pub fn save(path: &Path, layout: &Layout) -> Result<(), AppError> {
    layout.validate()?;
    let text = toml::to_string_pretty(layout)
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't save the layout."))?;
    write_atomic(path, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(id: &str, spec: Option<SessionSpec>) -> PaneNode {
        PaneNode::Leaf {
            pane_id: id.into(),
            spec,
            title: format!("title {id}"),
        }
    }

    fn sample() -> Layout {
        Layout {
            version: 1,
            active_tab: "t2".into(),
            groups: None,
            group_tabs: Default::default(),
            active_group: String::new(),
            tabs: vec![
                TabLayout {
                    id: "t1".into(),
                    title: "one".into(),
                    active: "a".into(),
                    view: None,
                    root: leaf(
                        "a",
                        Some(SessionSpec::Local {
                            shell_id: "powershell".into(),
                        }),
                    ),
                },
                TabLayout {
                    id: "t2".into(),
                    title: "two".into(),
                    active: "c".into(),
                    view: None,
                    root: PaneNode::Split {
                        direction: Direction::Row,
                        sizes: vec![0.25, 0.75],
                        children: vec![
                            leaf(
                                "b",
                                Some(SessionSpec::Wsl {
                                    distro: "Ubuntu".into(),
                                }),
                            ),
                            PaneNode::Split {
                                direction: Direction::Column,
                                sizes: vec![0.5, 0.5],
                                children: vec![leaf("c", None), leaf("d", None)],
                            },
                        ],
                    },
                },
            ],
        }
    }

    fn path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("layout.toml");
        (dir, path)
    }

    #[test]
    fn saves_and_reloads_tabs_splits_and_sessions() {
        let (_dir, path) = path();
        save(&path, &sample()).unwrap();
        assert_eq!(load(&path), Some(sample()));
    }

    #[test]
    fn no_file_means_start_fresh() {
        let (_dir, path) = path();
        assert_eq!(load(&path), None);
    }

    #[test]
    fn a_layout_with_no_tabs_means_start_fresh_and_is_not_treated_as_broken() {
        let (dir, path) = path();
        save(
            &path,
            &Layout {
                version: 1,
                active_tab: String::new(),
                groups: None,
                group_tabs: Default::default(),
                active_group: String::new(),
                tabs: vec![],
            },
        )
        .unwrap();
        assert_eq!(load(&path), None);
        assert!(!dir.path().join("layout.toml.bad").exists());
    }

    #[test]
    fn a_corrupt_file_is_kept_aside() {
        let (dir, path) = path();
        std::fs::write(&path, "tabs = [[[").unwrap();
        assert_eq!(load(&path), None);
        assert!(dir.path().join("layout.toml.bad").exists());
    }

    #[test]
    fn a_file_with_a_broken_tree_is_kept_aside() {
        let (dir, path) = path();
        let mut bad = sample();
        if let PaneNode::Split { sizes, .. } = &mut bad.tabs[1].root {
            sizes[0] = 0.9; // no longer adds up to 1
        }
        let text = toml::to_string_pretty(&bad).unwrap();
        std::fs::write(&path, text).unwrap();
        assert_eq!(load(&path), None);
        assert!(dir.path().join("layout.toml.bad").exists());
    }

    #[test]
    fn rejects_broken_layouts() {
        let mut a = sample();
        a.tabs[0].active = "missing".into();
        assert!(a.validate().is_err());

        let mut b = sample();
        if let PaneNode::Split { children, .. } = &mut b.tabs[1].root {
            children.pop(); // sizes no longer match children, and a split needs two
        }
        assert!(b.validate().is_err());

        let mut c = sample();
        c.tabs[0].root = PaneNode::Split {
            direction: Direction::Row,
            sizes: vec![1.0],
            children: vec![leaf("a", None)],
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn rejects_absurd_sizes_of_tree() {
        let mut deep = leaf("x", None);
        for _ in 0..(MAX_DEPTH + 2) {
            deep = PaneNode::Split {
                direction: Direction::Row,
                sizes: vec![0.5, 0.5],
                children: vec![deep, leaf("y", None)],
            };
        }
        let layout = Layout {
            version: 1,
            active_tab: "t".into(),
            groups: None,
            group_tabs: Default::default(),
            active_group: String::new(),
            tabs: vec![TabLayout {
                id: "t".into(),
                title: "t".into(),
                active: "x".into(),
                view: None,
                root: deep,
            }],
        };
        assert!(layout.validate().is_err());

        let many = Layout {
            version: 1,
            active_tab: String::new(),
            groups: None,
            group_tabs: Default::default(),
            active_group: String::new(),
            tabs: (0..=MAX_TABS)
                .map(|i| TabLayout {
                    id: i.to_string(),
                    title: "t".into(),
                    active: "p".into(),
                    view: None,
                    root: leaf("p", None),
                })
                .collect(),
        };
        assert!(many.validate().is_err());
    }

    #[test]
    fn saving_a_bad_layout_writes_nothing() {
        let (_dir, path) = path();
        let mut bad = sample();
        bad.tabs[0].active = "nope".into();
        assert!(save(&path, &bad).is_err());
        assert!(!path.exists());
    }
}

#[cfg(test)]
mod group_tests {
    use super::*;

    fn leaf(id: &str) -> PaneNode {
        PaneNode::Leaf {
            pane_id: id.into(),
            spec: None,
            title: String::new(),
        }
    }

    fn tab(id: &str, view: Option<&str>) -> TabLayout {
        TabLayout {
            id: id.into(),
            title: id.into(),
            active: "p".into(),
            root: leaf("p"),
            view: view.map(Into::into),
        }
    }

    #[test]
    fn panes_and_what_they_show_survive_a_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("layout.toml");
        let layout = Layout {
            version: 1,
            active_tab: "a".into(),
            tabs: vec![tab("a", None), tab("f", Some("files"))],
            groups: Some(PaneNode::Split {
                direction: Direction::Row,
                sizes: vec![0.4, 0.6],
                children: vec![leaf("g1"), leaf("g2")],
            }),
            group_tabs: [("g1".to_string(), "a".to_string()), ("g2".to_string(), "f".to_string())].into(),
            active_group: "g2".into(),
        };
        save(&path, &layout).unwrap();
        assert_eq!(load(&path), Some(layout));
    }

    #[test]
    fn a_pane_pointing_at_a_missing_tab_or_pane_is_refused() {
        let mut layout = Layout {
            version: 1,
            active_tab: "a".into(),
            tabs: vec![tab("a", None)],
            groups: Some(leaf("g1")),
            group_tabs: [("g1".to_string(), "nope".to_string())].into(),
            active_group: "g1".into(),
        };
        assert!(layout.validate().is_err());
        layout.group_tabs = [("zzz".to_string(), "a".to_string())].into();
        assert!(layout.validate().is_err());
        layout.group_tabs = [("g1".to_string(), "a".to_string())].into();
        assert!(layout.validate().is_ok());
    }
}
