/// popone ライブラリのエラー型
///
/// 公開API (`lib.rs`) で返すエラーを構造化する。
/// バイナリ側 (`main.rs`) では `anyhow` でラップして使用する。
#[derive(Debug, thiserror::Error)]
pub enum PoponeError {
    #[error("ファイル読み込み失敗: {0}")]
    Io(#[from] std::io::Error),

    #[error("GLB/VRM パース失敗: {0}")]
    GltfParse(#[from] gltf::Error),

    #[error("FBX パース失敗: {0}")]
    FbxParse(String),

    #[error("PMX パース失敗: {0}")]
    PmxParse(String),

    #[error("PMD パース失敗: {0}")]
    PmdParse(String),

    #[error("中間表現の抽出に失敗: {0}")]
    Extraction(String),

    #[error("PMX モデル構築失敗: {0}")]
    Build(String),

    #[error("テクスチャ書き出し失敗: {0}")]
    Texture(String),

    #[error("画像処理失敗: {0}")]
    Image(#[from] image::ImageError),

    #[error("unitypackage 展開失敗: {0}")]
    UnityPackage(String),

    #[error("アーカイブ処理失敗: {0}")]
    Archive(String),

    #[error("{0}")]
    Other(String),
}

/// anyhow::Error から PoponeError への変換（既存コードとの互換用）
impl From<anyhow::Error> for PoponeError {
    fn from(e: anyhow::Error) -> Self {
        PoponeError::Other(format!("{e:#}"))
    }
}

pub type Result<T> = std::result::Result<T, PoponeError>;
