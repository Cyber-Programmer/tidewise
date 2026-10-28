//! Package-manager and app-source adapters, one module each, behind a trait.
#![forbid(unsafe_code)]

pub mod appimage;
pub mod flatpak;
pub mod pacman;

/// An installed system package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub installed_size_bytes: u64,
    /// Packages that depend on this one. Empty when nothing does.
    pub required_by: Vec<String>,
}

/// Anything that can list installed packages. Read-only.
pub trait PackageSource {
    type Error;

    fn installed(&self) -> Result<Vec<Package>, Self::Error>;
}
