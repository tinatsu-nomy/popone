<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [更新履歴](#%E6%9B%B4%E6%96%B0%E5%B1%A5%E6%AD%B4)
  - [v0.2.32](#v0232)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD)
    - [コード品質・パフォーマンス改善](#%E3%82%B3%E3%83%BC%E3%83%89%E5%93%81%E8%B3%AA%E3%83%BB%E3%83%91%E3%83%95%E3%82%A9%E3%83%BC%E3%83%9E%E3%83%B3%E3%82%B9%E6%94%B9%E5%96%84)
  - [v0.2.31](#v0231)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-1)
    - [改善](#%E6%94%B9%E5%96%84)
  - [v0.2.30](#v0230)
    - [バグ修正](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3)
    - [改善](#%E6%94%B9%E5%96%84-1)
  - [v0.2.29](#v0229)
    - [バグ修正](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3-1)
    - [コード品質・パフォーマンス改善](#%E3%82%B3%E3%83%BC%E3%83%89%E5%93%81%E8%B3%AA%E3%83%BB%E3%83%91%E3%83%95%E3%82%A9%E3%83%BC%E3%83%9E%E3%83%B3%E3%82%B9%E6%94%B9%E5%96%84-1)
  - [v0.2.28](#v0228)
    - [バグ修正](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3-2)
    - [コード品質・パフォーマンス改善](#%E3%82%B3%E3%83%BC%E3%83%89%E5%93%81%E8%B3%AA%E3%83%BB%E3%83%91%E3%83%95%E3%82%A9%E3%83%BC%E3%83%9E%E3%83%B3%E3%82%B9%E6%94%B9%E5%96%84-2)
  - [v0.2.27](#v0227)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-2)
    - [バグ修正](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3-3)
    - [コード品質・パフォーマンス改善](#%E3%82%B3%E3%83%BC%E3%83%89%E5%93%81%E8%B3%AA%E3%83%BB%E3%83%91%E3%83%95%E3%82%A9%E3%83%BC%E3%83%9E%E3%83%B3%E3%82%B9%E6%94%B9%E5%96%84-3)
  - [v0.2.26](#v0226)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-3)
    - [バグ修正](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3-4)
    - [コード品質・パフォーマンス改善](#%E3%82%B3%E3%83%BC%E3%83%89%E5%93%81%E8%B3%AA%E3%83%BB%E3%83%91%E3%83%95%E3%82%A9%E3%83%BC%E3%83%9E%E3%83%B3%E3%82%B9%E6%94%B9%E5%96%84-4)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# 更新履歴

[English](CHANGELOG.md)

## v0.2.32

### 新機能

- **トゥーンテクスチャ個別生成（Phase 2）** — MToon/UTS2 材質で共有 toon01–toon10 の代わりに、`shade_color` → `diffuse` のグラデーション画像（256×16 PNG）を材質ごとに動的生成。生成画像は `textures/` に書き出し、`PmxToonRef::Texture(index)` で個別参照。非 MToon は `Shared(0)`、shade_color 無しは `Shared(2)` を維持。既存テクスチャとのファイル名衝突は `used_names` セットで回避し、書き出し後に PMX テクスチャパスを補正
- **OBJ/STL インポートオプションダイアログ** — ビューアで OBJ/STL ファイルを開く際にインポート設定ダイアログを表示。座標単位（mm / cm / m / inch）と Z-Up → Y-Up 変換の ON/OFF を選択可能。デフォルト値は従来のハードコード値と同一（OBJ: cm/Y-Up、STL: mm/Z-Up）。CLI は従来動作を維持

### コード品質・パフォーマンス改善

- **`path_ext_lower()` ユーティリティ** — 35箇所以上で重複していた `.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase()` パターンをクレートルートの `path_ext_lower()` 関数に統合。viewer/非 viewer 両ビルドからアクセス可能
- **カメラ bbox ヘルパー統合** — 4箇所の同一 bbox → camera メソッド呼び出しパターンを `camera_reset_to_model()` / `camera_fit_to_model()` ヘルパーメソッドに統合
- **`is_temp_path` キャッシュ** — `temp_dir()` の canonicalize と小文字文字列を `OnceLock` でキャッシュし、19箇所の呼び出しでの重複計算を排除
- **anyhow チェーン整理** — `ok_or_else(|| anyhow!(...))` パターンを `.context()` / `.with_context()` に統一（file_io.rs, main.rs, texture.rs）

## v0.2.31

### 新機能

- **Prefab `source_material` 照合（戦略1）** — FBX 抽出時に `GeometryInstance` を使用して各材質に `SourceMaterialRef`（renderer_path + slot_index）を設定。Prefab の renderer パスと正確にマッチし、材質名のあいまい一致に頼らないテクスチャマッピングを実現。三段階フォールバック: 戦略1（source_material）→ 戦略2（material_name）→ 戦略3（source_texture_name）

### 改善

- **`link_same_name` スコープ制限** — テクスチャ割り当ての「同名連動」機能を同一 `MaterialGroup`（同じモデルインスタンス内）に限定。従来は同じ FBX を2回 append して片方のテクスチャを変更するともう片方にも波及していたが、この問題を解消
- **リロード安定キー（`PkgModelLocator`）** — `.unitypackage` のリロード経路（`reload_archive_unitypackage`、`reload_append_unitypackage`）で `selected_pkg_model`（GUID/パスネームベース）によるモデル再選択を使用。ファイル名のみのマッチングから切り替え、同名ファイルが複数存在する場合（例: `Assets/A/Body.fbx` と `Assets/B/Body.fbx`）の取り違えを防止。VRM・append モデルも `PkgModelLocator` を保存して正確なリロードに対応
- **`resolve_pkg_model_for_cli`** — CLI 用モデル解決関数を追加。`--fbx-name` ヒントでパスネーム曖昧一致により FBX を選択し、候補リスト付きの構造化エラーメッセージを提供
- **`apply_resolved_textures` ヘルパー** — `embed_textures_with_prefab` のテクスチャ適用ロジック（ベーステクスチャ・ノーマルマップ・Emission）を共通ヘルパーに抽出し、戦略1と戦略2のコード重複を削減
- **`compute_geometry_world_transform` 削除** — `FbxScene.geometry_instances()` の `GeometryInstance.world_transform` に置き換え、重複したワールド変換計算を除去

## v0.2.30

### バグ修正

- **リロードキャンセル/失敗時にシェーダー設定が消失** — `ReloadSnapshot` に `DisplaySettings`（シェーダーオーバーライド・MMD パス・Auto シェーダー・ライト・Bloom 等）が保存・復元されていなかった問題を修正。リロードのキャンセルまたは失敗時に、現在のモデルに適用していたシェーダー設定が失われていた。`DisplaySettings` をスナップショットに含め、失敗時・成功時の両パスで復元するよう修正
- **新規モデルロードキャンセル時にシェーダー設定が消失** — バックグラウンドロード・同期ロードの両パスで、ロード開始前にシェーダー設定を事前リセットしていた問題を修正。新規ロードがキャンセルされた場合（アーカイブダイアログキャンセル・BG ロードキャンセル等）、前モデルのシェーダー設定が既に上書きされていた。シェーダーリセットを `finish_load_with_gpu`（成功時のみ通るパス）に遅延させ、キャンセル/失敗時は現在の表示設定を保持するよう修正

### 改善

- **選択ダイアログの移動可能化** — アーカイブ内モデル選択・UnityPackage モデル選択・FBX 読み込み選択・テクスチャ履歴上書き確認ダイアログをドラッグ移動可能に変更。`anchor(CENTER_CENTER)`（固定位置）から `default_pos(center) + pivot(CENTER_CENTER)`（初回は中央表示、ユーザーが移動可能）に変更。ダイアログを動かすことで背後のモデル一覧を確認可能に
- **`encase::ShaderType` の dead_code 警告抑制** — `CameraUniform` と `MaterialUniform` を `mod encase_uniforms { #![allow(dead_code)] }` サブモジュールに分離し、`encase` derive マクロが生成する 67 件の `function check is never used` 警告を抑制。`MmdMaterialUniform`（bytemuck のみ、`check` 関数生成なし）はモジュール外に維持。`pub use` で再エクスポート

## v0.2.29

### バグ修正

- **Nearest フィルタ使用時の異方性フィルタリングによるクラッシュ** — glTF サンプラーが `Nearest` フィルタリングを指定しているモデルのロード時にパニック（`Invalid filter mode for mipmapFilter: Nearest. When anisotropic clamp is not 1, all filter modes must be linear`）が発生する問題を修正。`anisotropy_clamp: 16` は 3 つのフィルタモード（mag, min, mipmap）が全て `Linear` の場合のみ適用し、それ以外は 1（デフォルト）にフォールバック

### コード品質・パフォーマンス改善

- **異方性テクスチャフィルタリング** — テクスチャサンプラー（`default_sampler`、`create_sampler_from_info`、`ensure_sampler`）に `anisotropy_clamp: 16` を追加。ミップマップ（v0.2.26）と組み合わせて斜め面のテクスチャシャープネスを向上。ハードウェア上限を超えた場合は GPU ドライバが自動クランプ
- **`TextureData` enum 化** — `IrTexture.data: Vec<u8>` + `mime_type == "image/x-raw-rgba8"` の文字列判定を型安全な `TextureData` enum（`Encoded(Vec<u8>)` / `RawRgba { pixels, width, height }`）に置換。`raw_dims: Option<(u32, u32)>` フィールドは `RawRgba` バリアントに吸収して除去。`is_raw_rgba()` メソッドは `matches!` ベースに変更。`TextureData` に `as_bytes()`、`len()`、`is_empty()` メソッドを追加し呼び出し側の変更を最小化
- **`CpuParseInput` enum 化** — `cpu_parse_model` の散在引数（`path`、`format`、`preloaded`）を `CpuParseInput::File { path, format, preloaded }` に集約し、関数名を `cpu_parse_source` にリネーム。将来のアーカイブ内パース BG 化に備えた `ArchiveEntry` / `Reload` バリアント拡張を想定した設計
- **ログのオンメモリ化** — ビューアのログ出力先を毎行のファイル I/O から `LogBuffer` 構造体（`data: Vec<u8>` + 累計カウンタ `total_written: usize` を `Arc<Mutex<…>>` で共有）に変更。バッファは 16MB 上限で先頭を切り詰め、`total_written` により drain 後も PMX 変換ログのオフセット整合性を保証。バッファは正常終了時およびパニック時にファイルにフラッシュ。CLI 変換は従来どおりファイル直書き
- **`encase` uniform バッファ移行** — `CameraUniform` と `MaterialUniform` を `bytemuck`（`#[repr(C)] #[derive(Pod, Zeroable)]`）から `encase::ShaderType` に移行。フィールド型を `[f32; 3/4]` / `[[f32; 4]; 4]` から `glam::Vec3/Vec4/Mat4` に変更。手動 `_pad` フィールド 8 個（`CameraUniform` 5 個、`MaterialUniform` 3 個）と対応する WGSL 宣言を除去。バッファシリアライズに `bytemuck::bytes_of` の代わりに `encase::UniformBuffer` を使用し、`GpuRenderer` 上の再利用 `Vec<u8>` で毎フレームの heap allocation を回避。`MmdMaterialUniform` と `Vertex` は `bytemuck` のまま維持（パディングフィールドなし、移行不要）

## v0.2.28

### バグ修正

- **アニメーションロードによるモデルロードの誤キャンセル** — `route_load_dispatch` がキャンセルを最優先で実行してから vrma/.anim/gltf アニメ/FBX アニメ単体などの intent を判定していたため、モデルロード進行中にアニメファイルを開くと先行モデルロードを潰した上でアニメ側も `self.loaded.is_none()` で失敗し両方失われていた問題を修正。intent 判定を先に行い、アニメーション単体要求と判定されたときは `bg_load` 進行中なら拒否メッセージを返して現行モデルロードを保護するように変更
- **キャンセル粒度の改善** — `cpu_parse_model` 内のキャンセルチェックが関数冒頭 1 回だけで、既にパースに入ったスレッドは最後まで CPU/I/O を使い切っていた問題を修正。各フォーマット分岐の冒頭、`read_data` / `load_glb` / `read_pmx` 等の重い I/O の後、`extract` 呼び出しの前後にチェック点を追加し、旧リクエストのキャンセルフラグが立った時点でディスパッチ境界で段階的に打ち切られるようにした

### コード品質・パフォーマンス改善

- **`BackgroundLoadState` enum 化** — `PendingState` の `load_dispatch: Option<PendingLoadDispatch>` と `bg_load: Option<BgLoadHandle>` の 2 フィールド併存を `bg_state: BackgroundLoadState` に統合。`Idle` / `PendingDispatch { dispatch, prior_loading }` / `Loading(BgLoadHandle)` の 3 バリアントで状態マシンを型レベル表現し、「両方 Some」「片方だけ取り残される」などの不正状態を排除。`PendingDispatch.prior_loading: Option<BgLoadHandle>` は Loading 中に新 dispatch が投入された場合の先行ハンドルを保持し、`route_load_dispatch` が intent（モデル要求 vs アニメ単体要求）に応じてキャンセル/保護を判断する。`BackgroundLoadState::submit_dispatch` ヘルパーで 4 つの dispatch 入口（ファイルダイアログ結果・D&D・IPC・コマンドライン引数）を統一した
- **バックグラウンドロードのキャンセル機構** — 新規ロード要求が来た時点で進行中の先行ロードをキャンセルできるよう、`Arc<AtomicBool>` ベースのキャンセルトークンを導入。v0.2.27 初期実装では先行ロード中の新規要求を**拒否**してエラーを出していたが、v0.2.28 からは先行ロードを**キャンセルして新規を受け付ける**形に変更（ただしアニメ単体要求など「先行ロード完了後のモデルに依存する」場合は拒否側に残す）。`cpu_parse_model` 内の複数箇所でキャンセルフラグをチェックし、セットされていれば `"bg load cancelled"` で即座に中断する。キャンセル由来エラーは UI に出さず `log::info!` のみで記録
- **バックグラウンドロードの世代管理（request_id）** — `BgLoadHandle { rx, cancel, request_id }` 構造体を新設し、`spawn_bg_load` 呼び出しごとに `ViewerApp.next_request_id` をインクリメントして発番。`BgLoadResult.request_id` と現世代のハンドルの `request_id` を受信時に突き合わせ、不一致なら古い世代の結果として破棄する。これにより「先行ロードがキャンセル直前にギリギリ完了して結果を送信していた」場合でも現世代のモデルが上書きされない
- **FBX reload 一時ディレクトリの競合回避** — Snapshot リロード時の FBX 外部テクスチャ展開先として使っていた固定名 `%TEMP%\popone_fbx_reload` を `tempfile::TempDir` に置換。毎回ユニーク名 (`popone_fbx_reload_XXXXXX`) を生成するため並行リロード時の競合が解消され、`Drop` で自動削除されるので明示的な `remove_dir_all` 呼び出しも不要になった

## v0.2.27

### 新機能

- **非同期モデル読み込み** — VRM / FBX / PMX / PMD / OBJ / STL / DirectX .x のモデルパースをバックグラウンドスレッドで実行し、UI フリーズを解消。従来はファイル選択後に 3D 表示まで数秒間 UI が固まっていた問題を根本解決。`std::thread::spawn` + `mpsc::channel` パターンで実装し、メインスレッドは毎フレーム `try_recv()` で結果をポーリング。「読み込み中...」オーバーレイが表示中もカメラ操作・ウィンドウ操作が継続可能
- **非同期ファイルダイアログ** — テクスチャ差し替え、モデル開く、モデル追加のファイルダイアログを非同期化。ダイアログ表示中も UI が応答する
- **生 RGBA テクスチャバイパス** — VRM/GLB の生ピクセルデータを PNG エンコードせずに直接 `IrTexture.data` に格納し、GPU アップロード時も PNG デコードをスキップ。`mime_type = "image/x-raw-rgba8"` + `raw_dims: Option<(u32, u32)>` で識別。4K テクスチャ × 多数マテリアルの VRM で PNG エンコード/デコードの往復を完全排除
- **ミップマップのバックグラウンド事前生成** — `IrTexture.mip_chain` フィールドを追加し、VRM 抽出時にミップチェーン（レベル 1 以降）をバックグラウンドスレッドで事前生成。メインスレッドは `queue.write_texture` で GPU に転送するだけ。KizunaAI_KAMATTE.vrm（26 テクスチャ、4K 解像度）で UI フリーズ時間が 7.6秒 → 0.5秒（15倍改善）

### バグ修正

- **IrTexture テストコンパイル修正** — `export_filter.rs` のテスト用 `IrTexture` 初期化リテラル 4 箇所で `source_path` フィールドが欠落し `cargo test --features viewer` がコンパイル不可だった問題を修正
- **archive 内 VRM/GLB の PNG 正規化漏れ** — `build_ir_from_archive_bundle` の VRM 分岐で `encode_ir_textures_as_png` の呼び出しが欠落していた問題を修正
- **PMD/PMX スフィアマップ読み取り回帰** — 非 temp ファイルロード時に `.sph`/`.spa` が空になりマゼンタフォールバックされていた問題を修正。`cpu_parse_model` が常に `collect_image_files_recursive` 経由で aux を集めていたが、同関数は `.sph`/`.spa` 拡張子を収集対象外としているため、スフィアマップが欠落していた。非 temp の PMD/PMX は `pmd_to_ir(path)` / `pmx_to_ir(pmx_dir)` でディスクから直接読む v0.2.26 以前のパスに戻し、temp/D&D の場合のみ `preloaded.aux_files` 経由の `*_with_aux` パスを使う形に修正
- **非同期ロード多重投入時の結果破棄** — ロード中にもう一度ロード要求を投入すると先行スレッドの受信チャネルが上書きされて完了結果が黙って破棄される問題を修正。`route_load_dispatch` 冒頭で `bg_load` が進行中なら新規 dispatch を拒否し、ユーザーにエラーメッセージを表示
- **非同期テクスチャダイアログの stale material index** — ダイアログを開いた後に別モデルをロードすると、保存していた `mat_idx` が新モデルの材質として誤適用 or panic する問題を修正。ダイアログ結果受信時に `mat_idx < loaded.ir.materials.len()` を検証し、範囲外なら破棄。さらに `finish_load_with_gpu` でモデル切替時に `pending_file_dialog` をクリア
- **DirectX .x テクスチャ Y 反転** — v0.2.24（DirectX .x サポート追加）以降、`.x` ファイルのテクスチャが上下逆に表示されていた問題を修正。`Vec2::new(tc.x, 1.0 - tc.y)` の Y 反転を削除。DirectX .x は D3D 慣習で UV (0,0) が左上原点のため、PMX/FBX と同じく反転不要（OBJ は OpenGL 慣習で左下原点のため反転が必要、という違い）
- **非 viewer ビルド回帰** — `cargo check` / `cargo test`（`--features viewer` 無し）が `could not find viewer in the crate root` でコンパイル不可だった問題を修正。`vrm/extract.rs` のミップマップ生成ヘルパーが `crate::viewer::texture::rgba8_to_linear_f32` / `linear_f32_to_rgba8` を参照していたが、`viewer` モジュールは `#[cfg(feature = "viewer")]` 配下のため CLI ビルドで壊れていた。sRGB↔linear LUT 変換ヘルパーを feature 非依存の新モジュール `crate::color` に切り出し、`vrm::extract`（CLI 経路）と `viewer::texture`（GPU 経路）の両方から利用する形に統合

### コード品質・パフォーマンス改善

- **ロード入口の統一** — `PendingLoadDispatch` 構造体を新設し、ファイルダイアログ結果・IPC 受信・D&D（temp 含む）・コマンドライン引数の全ロード入口を `pending.load_dispatch` 経由に統一。`self.preloaded` をグローバル状態から外し、dispatch 内に `preloaded: Option<PreloadedData>` として持たせる
- **後処理の集約** — `apply_bg_load_result` メソッドで direct ロード / append の後処理（アニメーションクリア、FBX 自動アニメーション適用、座標系互換チェック）を集約
- **`cpu_parse_model` フリー関数** — `try_load_*` メソッドの CPU パース部分を `&self` を取らないフリー関数として抽出。バックグラウンドスレッドから安全に呼び出せる
- **`route_load_dispatch` ルーティング** — メインスレッドでフォーマット検出・アニメ判定・FBX choice ダイアログ・アーカイブ/UnityPackage 同期フォールバックを振り分け、通常のモデル読み込みのみバックグラウンドに送る
- **sRGB↔linear 変換の LUT 化** — `srgb_to_linear`（256 エントリ）・`linear_to_srgb`（4096 エントリ）を `OnceLock` で遅延初期化するルックアップテーブルに置き換え、`powf` 呼び出しを排除。ミップマップ生成時の色空間変換が数倍高速化

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
