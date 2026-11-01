//! Package-manager and app-source adapters, one module each, behind a trait.
#![forbid(unsafe_code)]

pub mod appimage;
pub mod apt;
pub mod cleanup;
pub mod dnf;
pub mod flatpak;
pub mod journal;
pub mod junk;
pub mod pacman;
pub mod snap;
pub mod source;

/// An installed system package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub installed_size_bytes: u64,
    /// Packages that depend on this one. Empty when nothing does.
    pub required_by: Vec<String>,
}
