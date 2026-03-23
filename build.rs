fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    // アイコンリソース埋め込み（ホストが Windows の場合のみ: rc.exe が必要）
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/popone_icon.ico");
        res.compile().expect("exe アイコンリソースのコンパイル失敗");
    }

    // ビューア有効時はスタックサイズを 8 MB に拡大
    // (eframe/winit/wgpu のコールバックチェーンが深く、デフォルト 1 MB では不足する場合がある)
    if std::env::var("CARGO_FEATURE_VIEWER").is_ok() {
        let target = std::env::var("TARGET").unwrap_or_default();
        if target.contains("msvc") {
            println!("cargo:rustc-link-arg=/STACK:8388608");
        } else if target.contains("windows-gnu") {
            println!("cargo:rustc-link-arg=-Wl,--stack,8388608");
        }
    }
}
