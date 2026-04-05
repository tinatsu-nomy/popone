<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [更新履歴](#%E6%9B%B4%E6%96%B0%E5%B1%A5%E6%AD%B4)
  - [v0.2.26](#v0226)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD)
    - [バグ修正](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3)
    - [コード品質・パフォーマンス改善](#%E3%82%B3%E3%83%BC%E3%83%89%E5%93%81%E8%B3%AA%E3%83%BB%E3%83%91%E3%83%95%E3%82%A9%E3%83%BC%E3%83%9E%E3%83%B3%E3%82%B9%E6%94%B9%E5%96%84)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# 更新履歴

[English](CHANGELOG.md)

## v0.2.26

### 新機能

- **ミップマップ生成** — テクスチャアップロード時にミップチェーンを自動生成。ミップレベル数は `floor(log2(max(w,h))) + 1` で計算し、linear 色空間で縮小（sRGBデコード → リサイズ → sRGBエンコード）することで物理的に正しいブレンドを実現。カメラを引いた際のモアレ・エイリアシングを解消。NPOT テクスチャにも対応
- **テクスチャ割当トレース** — 全テクスチャの割当を `source_path` 付きでログ出力。出自は `embedded`、`prefab(名前.prefab): Assets/...`、`archive(名前.zip): ファイル名`、ファイルパスで表示。テクスチャ割当問題のトラブルシュートに対応
- **ファイルオープン・モデル選択ログ** — `Open file`、`Append file`、`Load from archive`、`Unitypackage indexed`、`Archive indexed`、`Model loaded` イベントをログ記録し、完全なトレーサビリティを実現

### バグ修正

- **UnityPackage テクスチャ分離** — アーカイブ経由の FBX ロードで近傍テクスチャ検索を無効化（`fbx_path=None`）。無関係なフォルダのテクスチャ誤割当を防止
- **UNC パス正規化** — `\\?\UNC\server\share` パスを `\\server\share` に正しく正規化（以前は `UNC\server\share` になっていた）
- **MMD シェーダー定数** — MMD シェーダーマクロに `ALPHA_DISCARD_THRESHOLD` 定数を追加（マジックナンバーリファクタリング後の追加漏れで PMX/PMD ロード時クラッシュ）
- **ダークテーマ永続化** — `setup_dark_theme()` を `update()` 初回フレームでフラグ付き再適用。eframe の初期化後スタイルリセットを回避

### コード品質・パフォーマンス改善

- **ダークテーマ初期化** — `setup_dark_theme()` を毎フレーム呼び出しから起動時1回のみに変更。フレーム毎の `Style` クローン + `set_style()` オーバーヘッドを排除
- **モーフバッファ再利用** — `apply_morphs_to_buf` が毎回 `Vec<bool>` をヒープアロケーションしていたのを、既存の `morph_visited` バッファを `fill(false)` + `resize()` で再利用する方式に変更
- **WGSL シェーダー重複解消** — `wgsl_mtoon_bindings!`・`wgsl_mtoon_helpers!`・`wgsl_fs_outline!` マクロを導入し、メインシェーダーとアウトラインシェーダー間のコピペ重複を解消（-107行）。sRGB/Unorm バリアントはパラメータ化
- **MaterialParams 構造体化** — `create_material_bind_group` の43個の位置引数を名前付き `MaterialParams` 構造体に集約（3引数）。引数順序ミスによるバグを防止
- **unwrap() 排除** — 本番コードの `unwrap()` を全て排除（53箇所以上）。描画パスは `if let` ガード + 描画スキップでフォールバック、パーサーは `expect()` で不変条件を明示またはエラー伝搬
- **Bloom BindGroup キャッシュ** — Bloom パスの BindGroup をキャッシュし、offscreen テクスチャ変更時（リサイズ/MSAA切替）のみ再作成。毎フレーム2回の `create_bind_group` を排除
- **Bloom 中間バッファ精度向上** — Bloom のダウンサンプル/アップサンプルチェーンを `Rgba8Unorm`（8bit、256段階）から `Rgba16Float` に変更。HDR emissive のグラデーションでのバンディングを解消
- **半透明ソートキャッシュ** — カメラ eye 位置・頂点バッファ・DrawCall 数が前フレームと同一の場合、重心再計算+ソートをスキップ
- **スキニング座標事前変換** — 頂点毎の PMX→glTF→スキン→PMX 三段変換（6回/頂点/フレーム）を、ボーン毎の `M*delta*M` 共役変換に置き換え。頂点ループ内の座標変換を全排除
- **render_to_texture 分割** — 835行の巨大関数を6つのヘルパーメソッド（`build_camera_uniform`、`build_draw_queue`、`draw_standard_meshes`、`draw_mmd_meshes`、`draw_highlight`、`draw_overlays`）+ 265行のオーケストレータに分割
- **MaterialDisplayState 構造体** — 材質毎の4本の `Vec<bool>`（smooth_normals, clear_normals, normal_map, bloom）を `Vec<MaterialDisplayState>` に集約
- **DynamicBuffer 構造体** — 7つの可視化バッファの3つ組（buf/capacity/vertex_count）を `DynamicBuffer` 構造体に集約し、共通 `upload()` メソッドで統一
- **パイプライン遅延生成** — 起動時の4パイプラインセット（100本以上）一括コンパイルを廃止。必要なセットのみ初回使用時に `ensure_pipelines()` で生成
- **ReloadSnapshot** — `reload()` の20以上の手動退避/復元フィールドを `ReloadSnapshot` 構造体に集約。`save`/`restore_on_success`/`restore_on_failure` の対称メソッドで管理
- **Named Pipe 堅牢化** — パイプバッファを 4KB → 32KB に拡大。`canonicalize` パスの `\\?\` プレフィックスを除去（UNC パスは `\\server\share` に正しく正規化）
- **bone_children Clone 排除** — `SkinningData.bone_children` フィールドを削除し、`IrBone.children` を直接参照。モデルロード時の200回以上のヒープアロケーションを排除
- **ファジーボーンマッチ O(n) 化** — `bone_name_to_idx.values().any()` の O(n²) 探索を `HashSet<usize>` の O(1) 判定に置き換え
- **create_pipeline_set 簡素化** — 14個の位置引数を `&self` メソッド（3引数: `device`, `use_unorm`, `msaa`）に変更
- **Reverse-Z デプスバッファ** — デプスクリア 1.0→0.0、比較 Less→Greater、射影 near/far 入替。遠方のデプス精度が劇的に改善し、巨大モデルでの Z-fighting を解消
- **グリッド整数ループ化** — 浮動小数点加算ループ (`x += step`) を整数インデックスループに変更し、巨大グリッドでの蓄積誤差を排除
- **WGSL PI 定数化** — ハードコード `3.14159`（5桁）を `const PI: f32 = 3.14159265`（8桁）に変更
- **マジックナンバー排除** — MMD ambient スケール、エッジオフセット、ボーン表示半径、球体セグメント数、ダークテーマ色を名前付き定数化（Rust + WGSL）
- **FileFormat enum** — ファイル拡張子判定を `detect_format()` に一元化し、4箇所の分散 match ブロックを統一
- **bool-to-f32 ヘルパー** — `b2f()` 関数で `if x { 1.0 } else { 0.0 }` パターン9箇所を統一
- **pos_fn ユーティリティ** — `coord::pos_fn(is_vrm0)` で VRM0/VRM1 座標関数選択パターン4箇所を統一
- **toonテクスチャ圧縮** — 100行のハードコードRGBデータを `toon_step()`/`toon_rle()` const fn で約45行に圧縮
- **エラーチェーン保持** — `ResultExt::context()` を `WithContext` バリアントで元エラーの `source()` チェーンを構造的に保持する方式に変更。`PoponeError::Anyhow` バリアントで `anyhow::Error` を構造的に保持
- **表情チャネル事前マッピング** — `apply_expressions` の毎フレーム HashMap 走査を事前構築済み `Vec<(String, usize)>` マッピングに置き換え
- **`#[expect(dead_code)]`** — 5箇所の `#[allow(dead_code)]` を `#[expect(dead_code)]` に変更、1箇所は実使用コードと判明し削除
- **format_number 最適化** — 二重反転 char イテレーションを先頭からの単一パスに変更
- **ログメッセージ英語化** — 全 `log::info/warn/error/debug` メッセージを英語に統一（検索容易性）。UI 表示文言（`ConvertMessage`）は日本語を維持
