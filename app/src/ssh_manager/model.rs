//! In-memory SSH host model.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A connectable SSH host: ssh_config-derived fields + optional annotation.
#[derive(Debug, Clone)]
pub struct SshHost {
    pub alias: String,
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<PathBuf>,
    pub proxy_jump: Option<String>,
    pub source: HostSource,
    pub annotation: Option<HostAnnotation>,
}

/// Where a host's ssh_config block is defined.
///
/// Determines whether Warp can rewrite it via the in-app editor. `User`-source
/// hosts are read-only inside Warp; the user opens their editor to change them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostSource {
    /// Defined inside `~/.ssh/config` (or any file it `Include`s).
    User,
    /// Defined inside `~/.warp/ssh_config` (excluding its leading `Include`).
    Warp,
}

/// User-visible annotation, persisted in the Warp DB and joined onto the
/// parsed host by alias. Independent of ssh_config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostAnnotation {
    pub tags: Vec<String>,
    pub color: Option<ColorSlug>,
    pub notes: Option<String>,
    pub last_connected_at: Option<DateTime<Utc>>,
}

/// Fixed palette of colors users can assign to a host. Mapped to hex by the
/// renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorSlug {
    Red,
    Orange,
    Green,
    Blue,
    Purple,
    Gray,
}

impl ColorSlug {
    /// Hex code for rendering (no leading `#`).
    pub fn hex(self) -> &'static str {
        match self {
            ColorSlug::Red => "e74c3c",
            ColorSlug::Orange => "f39c12",
            ColorSlug::Green => "2ecc71",
            ColorSlug::Blue => "3498db",
            ColorSlug::Purple => "9b59b6",
            ColorSlug::Gray => "777777",
        }
    }

    /// All slugs in display order. Useful for rendering the picker.
    pub fn all() -> &'static [ColorSlug] {
        &[
            ColorSlug::Red,
            ColorSlug::Orange,
            ColorSlug::Green,
            ColorSlug::Blue,
            ColorSlug::Purple,
            ColorSlug::Gray,
        ]
    }

    pub fn as_slug(self) -> &'static str {
        match self {
            ColorSlug::Red => "red",
            ColorSlug::Orange => "orange",
            ColorSlug::Green => "green",
            ColorSlug::Blue => "blue",
            ColorSlug::Purple => "purple",
            ColorSlug::Gray => "gray",
        }
    }

    pub fn from_slug(s: &str) -> Option<ColorSlug> {
        match s {
            "red" => Some(ColorSlug::Red),
            "orange" => Some(ColorSlug::Orange),
            "green" => Some(ColorSlug::Green),
            "blue" => Some(ColorSlug::Blue),
            "purple" => Some(ColorSlug::Purple),
            "gray" => Some(ColorSlug::Gray),
            _ => None,
        }
    }
}
