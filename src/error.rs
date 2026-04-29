//! popone library error type.
//!
//! Structured errors returned from the public API (`lib.rs`). The binary
//! side (`main.rs`) wraps these with `anyhow` for terminal display.
//!
//! Each variant's user-facing prefix is resolved through `rust-i18n`'s
//! `t!()` macro at format time, so the same `PoponeError` value renders
//! in the active locale (ja / en / zh) without needing to be reconstructed.

use rust_i18n::t;

#[derive(Debug, thiserror::Error)]
pub enum PoponeError {
    #[error("{}: {}", t!("error.io_failed"), .0)]
    Io(#[from] std::io::Error),

    #[error("{}: {}", t!("error.gltf_parse_failed"), .0)]
    GltfParse(#[from] gltf::Error),

    #[error("{}: {}", t!("error.fbx_parse_failed"), .0)]
    FbxParse(String),

    #[error("{}: {}", t!("error.pmx_parse_failed"), .0)]
    PmxParse(String),

    #[error("{}: {}", t!("error.pmd_parse_failed"), .0)]
    PmdParse(String),

    #[error("{}: {}", t!("error.obj_parse_failed"), .0)]
    ObjParse(String),

    #[error("{}: {}", t!("error.stl_parse_failed"), .0)]
    StlParse(String),

    #[error("{}: {}", t!("error.directx_parse_failed"), .0)]
    DirectXParse(String),

    #[error("{}: {}", t!("error.extraction_failed"), .0)]
    Extraction(String),

    #[error("{}: {}", t!("error.build_failed"), .0)]
    Build(String),

    #[error("{}: {}", t!("error.texture_failed"), .0)]
    Texture(String),

    #[error("{}: {}", t!("error.image_failed"), .0)]
    Image(#[from] image::ImageError),

    #[error("{}: {}", t!("error.unitypackage_failed"), .0)]
    UnityPackage(String),

    #[error("{}: {}", t!("error.archive_failed"), .0)]
    Archive(String),

    #[error("{0}")]
    Other(String),

    /// Error with a contextual message attached. Created via
    /// `ResultExt::context()` / `with_context()`; the original error
    /// chain is preserved through `source()`.
    #[error("{context}")]
    WithContext {
        context: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Carries an `anyhow::Error` chain verbatim. Constructed
    /// automatically via `From<anyhow::Error>`.
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl From<zip::result::ZipError> for PoponeError {
    fn from(e: zip::result::ZipError) -> Self {
        PoponeError::Archive(format!("{e}"))
    }
}

impl From<sevenz_rust2::Error> for PoponeError {
    fn from(e: sevenz_rust2::Error) -> Self {
        PoponeError::Archive(format!("{e}"))
    }
}

impl From<crate::unitypackage::PkgError> for PoponeError {
    fn from(err: crate::unitypackage::PkgError) -> Self {
        PoponeError::UnityPackage(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, PoponeError>;

/// Helper trait equivalent to `anyhow::Context`.
/// Provides `.context("msg")` / `.with_context(|| "msg")` for `Result<T, E>`.
pub trait ResultExt<T> {
    fn context(self, msg: &str) -> Result<T>;
    fn with_context<F: FnOnce() -> String>(self, f: F) -> Result<T>;
}

impl<T, E: std::error::Error + Send + Sync + 'static> ResultExt<T> for std::result::Result<T, E> {
    fn context(self, msg: &str) -> Result<T> {
        self.map_err(|e| PoponeError::WithContext {
            context: msg.to_string(),
            source: Box::new(e),
        })
    }
    fn with_context<F: FnOnce() -> String>(self, f: F) -> Result<T> {
        self.map_err(|e| PoponeError::WithContext {
            context: f(),
            source: Box::new(e),
        })
    }
}

impl<T> ResultExt<T> for Option<T> {
    fn context(self, msg: &str) -> Result<T> {
        self.ok_or_else(|| PoponeError::Other(msg.to_string()))
    }
    fn with_context<F: FnOnce() -> String>(self, f: F) -> Result<T> {
        self.ok_or_else(|| PoponeError::Other(f()))
    }
}
