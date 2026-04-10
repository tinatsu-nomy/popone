<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [更新履歴](#%E6%9B%B4%E6%96%B0%E5%B1%A5%E6%AD%B4)
  - [v0.2.41](#v0241)
    - [改善](#%E6%94%B9%E5%96%84)
    - [ドキュメント・ビルド](#%E3%83%89%E3%82%AD%E3%83%A5%E3%83%A1%E3%83%B3%E3%83%88%E3%83%BB%E3%83%93%E3%83%AB%E3%83%89)
  - [v0.2.40](#v0240)
    - [セキュリティ](#%E3%82%BB%E3%82%AD%E3%83%A5%E3%83%AA%E3%83%86%E3%82%A3)
    - [改善](#%E6%94%B9%E5%96%84-1)
    - [バグ修正](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3)
    - [コード品質](#%E3%82%B3%E3%83%BC%E3%83%89%E5%93%81%E8%B3%AA)
  - [v0.2.39](#v0239)
    - [パフォーマンス](#%E3%83%91%E3%83%95%E3%82%A9%E3%83%BC%E3%83%9E%E3%83%B3%E3%82%B9)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD)
    - [バグ修正](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3-1)
    - [アーキテクチャ](#%E3%82%A2%E3%83%BC%E3%82%AD%E3%83%86%E3%82%AF%E3%83%81%E3%83%A3)
    - [リファクタリング](#%E3%83%AA%E3%83%95%E3%82%A1%E3%82%AF%E3%82%BF%E3%83%AA%E3%83%B3%E3%82%B0)
  - [v0.2.38](#v0238)
    - [パフォーマンス](#%E3%83%91%E3%83%95%E3%82%A9%E3%83%BC%E3%83%9E%E3%83%B3%E3%82%B9-1)
    - [改善](#%E6%94%B9%E5%96%84-2)
  - [v0.2.37](#v0237)
    - [バグ修正](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3-2)
  - [v0.2.36](#v0236)
    - [改善](#%E6%94%B9%E5%96%84-3)
  - [v0.2.35](#v0235)
    - [改善](#%E6%94%B9%E5%96%84-4)
    - [ドキュメント](#%E3%83%89%E3%82%AD%E3%83%A5%E3%83%A1%E3%83%B3%E3%83%88)
  - [v0.2.34](#v0234)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-1)
    - [改善](#%E6%94%B9%E5%96%84-5)
  - [v0.2.33](#v0233)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-2)
    - [改善](#%E6%94%B9%E5%96%84-6)
  - [v0.2.32](#v0232)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-3)
    - [コード品質・パフォーマンス改善](#%E3%82%B3%E3%83%BC%E3%83%89%E5%93%81%E8%B3%AA%E3%83%BB%E3%83%91%E3%83%95%E3%82%A9%E3%83%BC%E3%83%9E%E3%83%B3%E3%82%B9%E6%94%B9%E5%96%84)
  - [v0.2.31](#v0231)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-4)
    - [改善](#%E6%94%B9%E5%96%84-7)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# 更新履歴

[English](CHANGELOG.md)

## v0.2.41

### 改善

- **中国語テキスト表示対応** — `NotoSansSC-Regular.otf`（簡体字中国語）をフォールバックフォントとして追加。中華圏 VRM/FBX モデルのモデル名・作者名等のメタ情報が □（豆腐）にならず正しく表示される。フォントフォールバック順: JP → SC → egui デフォルト
- **モデルロード時に情報タブへ自動切替** — 新規モデルロード時にサイドパネルを自動的に情報タブに切り替え、モデルのメタ情報をすぐに確認可能に。追加読み込み（append）時はタブを維持
- **テーマカラーの設定** — `popone.toml` に `[theme]` セクションを追加。6 色を変更可能: `panel_bg`（パネル背景）、`border`（ボーダー）、`accent`（ホバー・選択）、`text`（テキスト）、`widget_bg`（ウィジェット背景）、`active`（クリック中）。値は 6 桁 hex 文字列（例: `"4A90D9"` / `"#4A90D9"`）。未指定の項目はデフォルトのダークテーマにフォールバック

### ドキュメント・ビルド

- **サードパーティライセンス明記** — 同梱フォント（NotoSansJP + NotoSansSC）の SIL Open Font License を記載した `THIRD_PARTY_NOTICES.md` を追加。両 README のライセンスセクションからも参照し、コードのライセンス（0BSD）と同梱アセットのライセンスを明確に分離
- **GitHub Actions CI** — `.github/workflows/ci.yml` を追加。Windows ランナーでフォーマットチェック（`cargo fmt`）、clippy lint（`-D warnings`）、ビルド（CLI + ビューア）、テストスイートを実行
- **ソースビルド手順の文書化** — 両 README にビルド手順を追加。exe アイコン埋め込みに必要な Windows SDK の推奨を明記
- **アイコン埋め込みの堅牢化** — `build.rs` で Windows SDK（`rc.exe`）が未インストールの場合にパニックせず、警告を出力してアイコンなしでビルドを継続

## v0.2.40

### セキュリティ

- **テクスチャパストラバーサル防止** — `sanitize_rel_path()` がテクスチャ相対パスから `..` コンポーネントおよび Windows ドライブレター（`C:` 等）を除去してからモデルのベースディレクトリと結合する。DirectX .x / OBJ / PMX / PMD の4フォーマットのディスク直接読み込みパスすべてに適用。アーカイブ経由は既存の `normalize_archive_path()` で保護済み。悪意あるモデルファイルによるモデルディレクトリ外の任意ファイル読み込みを防止
- **絶対パスすり抜け防止** — `sanitize_rel_path()` が `:` を含むパスコンポーネント（Windows ドライブレター）も除去し、`C:/secret.png` のような絶対パスによる `base_dir.join()` 制約のバイパスを防止

### 改善

- **設定ファイルの保存先を `%LOCALAPPDATA%\popone` に移行** — 設定（`popone.toml`）・テクスチャ履歴（`popone_history.json`）・ログの保存先を Windows では `%LOCALAPPDATA%\popone` に変更。読み取り専用の場所（`Program Files` 等）にインストールした場合の書き込みエラーを防止。初回起動時に既存ファイルを自動マイグレーション。非 Windows プラットフォームでは従来通り exe 隣接にフォールバック
- **7z メモリピーク削減** — 7z アーカイブからエントリを展開後、ソースファイルが永続（非temp）の場合は元の圧縮データを即時解放。従来は圧縮データと展開済みデータが同時にメモリ上に保持されていた
- **ログレベル・保持数の設定** — `popone.toml` に `[log]` セクションを追加。`level`（error/warn/info/debug、デフォルト: debug）と `keep`（ログファイル保持数、デフォルト: 5）を設定可能。設定はロガー初期化前に読み込まれるため即座に反映

### バグ修正

- **シェーダー [Auto] が手動変更後に復元されない** — シェーダーを Auto から別のモード（Unlit 等）に変更後、Auto に戻しても元の Auto 選択シェーダーに復元されなかった。`set_shader_selection(Auto)` は `auto_shader = true` を設定するが `use_mmd_path` を再計算しておらず、前回のオーバーライド時の値が残っていた。UI でのシェーダー選択変更後に `normalize_shader_state()` を呼び出すよう修正
- **HDR エミッシブ材質がデフォルト OFF になる** — `emissive_factor` の成分が 1.0 を超える材質（`KHR_materials_emissive_strength` 使用時）の材質別エミッシブトグルがデフォルト OFF で初期化されていた。Seed-san 等の VRM 1.0 モデルでロード時にエミッションが無効になっていた。HDR 自動検出を削除し、全材質をデフォルト ON に変更

### コード品質

- **Clippy クリーン (`-D warnings`)** — 全96件の clippy 警告を解消: 57件自動修正、25件手動修正（イテレータパターン化、`copy_from_slice`、大サイズ enum バリアントの `Box` 化、構造体リテラル初期化）、12件の構造的警告を `#[allow]` で抑制（`too_many_arguments`、`type_complexity`）

## v0.2.39

### パフォーマンス

- **UnityPackage/アーカイブの非同期読み込み** — `.unitypackage` および ZIP/7z のファイル読み込み・`build_unity_package_index`・`list_models` をバックグラウンドスレッドで実行し、インデックス構築中のUIフリーズを解消
- **パッケージ内モデルの非同期パース** — `.unitypackage` 内 FBX/VRM/Prefab のCPUパースとアーカイブ内モデルの解凍+パースをバックグラウンドスレッドで実行（`spawn_bg_pkg_load` / `spawn_bg_archive_load`）。従来はUIスレッドで同期実行
- **テクスチャ事前デコード** — `pre_decode_textures` がバックグラウンドスレッドで `TextureData::Encoded`（PNG/JPEG/TGA/PSD）を `TextureData::RawRgba` に変換。メインスレッドの `upload_textures_from_ir` から画像デコードコストを排除
- **GPUテクスチャアップロードのフレーム分割** — `PendingGpuBuild` が1フレームあたり4枚ずつ `upload_single_texture` でGPUにアップロード。大量テクスチャでのメインスレッドブロックを防止。初回ロード・追加読み込みの両方に適用
- **テクスチャデータのゼロコピー** — `take_fbx_and_textures` / `take_vrm` の戻り値を `Vec<u8>` → `Arc<[u8]>` に変更。`embed_textures_into_ir` を `AsRef<[u8]>` でジェネリック化。`pkg_textures` およびリロードパスを `Arc<[u8]>` に統一し、不要な `.to_vec()` コピーを排除
- **D&D 一時ファイル先読みの非ブロック化** — `process_drag_and_drop` 内の `std::fs::read` / `collect_image_files_recursive` を削除。ファイル読み込みは BG パーススレッドの `read_data` / `collect_aux` クロージャに委譲
- **PMX 変換のバックグラウンド化** — `execute_conversion` が IR を clone して BG スレッドで `convert_ir_to_pmx_with_cancel` を実行。UI スレッドは即座に復帰し「PMX変換中...」オーバーレイとキャンセルボタンを表示
- **リロードのバックグラウンド化（File/Snapshot）** — `reload_current` が File・Snapshot ソースを既存の `spawn_bg_load` パイプライン経由でディスパッチ。Archive/UnityPackage は同期パスを維持。リロードスナップショットを GPU ビルド完了後に復元
- **`TextureData::RawRgba` Arc 共有** — `pixels` フィールドを `Vec<u8>` → `Arc<[u8]>` に変更。`mip_chain` エントリも `Vec<u8>` → `Arc<[u8]>`。PMX 変換用の IrModel clone 時、テクスチャデータのコピーコストがほぼゼロに
- **`IrModel::clone_for_export`** — GPU 専用データ（`mip_chain`・`uvs1`）を除外した軽量 clone。PMX 変換 BG スレッド起動時の UI スレッドコピーコストを最小化
- **GPU パイプラインのスプラッシュ画面ウォームアップ** — `GpuRenderer::new()`（シェーダーコンパイル）と `ensure_pipelines()`（26 パイプライン × 4 構成）をスプラッシュ画面表示中に `WarmupPhase` ステートマシンで 1 フェーズ/フレームずつ段階実行。初回モデルロードの ~10s フリーズを解消（リリースビルド実測: 合計 76ms）
- **GPU モデルビルドの CPU オフロード** — `build_gpu_model_inner` を `cpu_prep_model`（頂点 dedup・法線平均化・モーフ前計算 → BG スレッド実行）と `gpu_finalize_model`（バッファ/bind group 生成 → メインスレッドで <7ms）に分離。`PendingGpuBuild` を 3 フェーズ（テクスチャ upload → BG cpu_prep → GPU finalize）に拡張
- **pkg サムネイルキャッシュの差分更新** — `apply_pkg_append_post` で `rebuild_pkg_thumb_cache()`（全サムネイル再生成）の代わりに `append_pkg_thumb_cache(start_index)` を使用し、新規追加分のみサムネイル生成。バッチ append 時の累積フリーズ増加を解消（改善前: 15s→61s 累積増加、改善後: ~7.6s 一定）

### 新機能

- **中止ボタン拡張** — プログレスオーバーレイの「中止」ボタンを BGロード中・GPU構築中・PMX変換中の全フェーズで表示。Escキーでもキャンセル可能。リロード中のキャンセルでは旧モデルを復元（初期状態に戻さない）
- **選択ダイアログの Esc キー対応** — FBX 読み込み方法選択・OBJ/STL インポート設定・UnityPackage モデル選択・アーカイブ内モデル選択の各ダイアログを Esc キーで閉じられるように
- **FBX選択ダイアログのバックグラウンド化** — FBXモデル/アニメーション選択ダイアログ（`execute_fbx_choice`）の確定後のロードを `spawn_bg_load` / `spawn_bg_pkg_load` 経由に変更
- **PMX 変換の協調キャンセル** — `convert_ir_to_pmx_with_cancel` が各ステップ間（テクスチャ書き出し・PMX 構築・ファイル書き込み）でキャンセルフラグを確認。テクスチャ書き出しは1枚ごとに確認。全出力を一時ディレクトリ（`.popone_convert_tmp/`）に書き出し、成功時のみ最終パスに移動。キャンセル時は一時ディレクトリごと削除

### バグ修正

- **PMX 出力ファイル名の切り詰め** — `sanitize_filename` でモデル名が 80 文字を超える場合に Unicode 文字境界で切り詰めるように修正。従来はVRM モデルのメタデータ名（`meta.name`）が非常に長い場合、Windows のパス長制限を超過したりファイルシステムエラーが発生する可能性があった

### アーキテクチャ

- **`CpuParseInput` 拡張** — `ArchiveModel`・`PkgModel`・`UnityPackageIndex`・`ArchiveIndex` バリアントを追加し、全ロードパスのバックグラウンド処理に対応
- **`BgLoadKind` 拡張** — `ArchiveInitial`・`ArchiveAppend`・`ArchivePreparedUnityPackage`・`PkgInitial`・`PkgAppend`・`NeedsFbxChoice`・`UnityPackageIndexed`・`ArchiveIndexed` を追加。`Box<Payload>` 構造体でリクエストとレスポンスのデータを分離
- **`PendingGpuBuild` ステートマシン** — GPUテクスチャアップロードをフレーム分割（4枚/フレーム）。BGロード結果には `start_deferred_gpu_build` を使用。リロードの File/Snapshot ソースもこのパイプラインを使用
- **Append GPU ビルド遅延化** — 追加読み込みも `start_deferred_append_gpu_build_ext` でフレーム分割。GPUビルド失敗時はIR truncate + 旧GPUモデルでロールバック
- **`build_ir_from_archive_bundle` フリー関数化** — `&self` メソッドからフリー関数に抽出し、バックグラウンドスレッドから呼び出し可能に
- **`PendingConvertBg`** — BG PMX 変換状態。`mpsc::Receiver` で結果ポーリング、`AtomicBool` でキャンセル。`process_pending_tasks` で監視
- **`reload_snapshot` フィールド** — `ViewerApp` が BG リロード中に `ReloadSnapshot` を保持。GPU ビルド完了時に `finish_reload_from_snapshot` で復元。キャンセル時は `restore_snapshot_on_failure` で旧モデルを保持
- **`IrModel` / `IrMesh` / `IrPhysics` Clone 導出** — `clone_for_export` および BG PMX 変換を可能にするため `Clone` derive を追加
- **`watchdog.rs` — メインスレッド応答性監視** — バックグラウンドのウォッチドッグスレッドがメインスレッドのハートビート（`AtomicU64` エポックミリ秒、閾値 5 秒、チェック間隔 2 秒）を監視。閾値内にハートビート更新がなければ `[watchdog] Main thread unresponsive` をログ出力し、復帰時にフリーズ総時間を記録。最小化中は `PAUSED` 番兵値（`u64::MAX`）で誤検知を抑制。idle 時は `request_repaint_after(3s)` でハートビートを維持
- **`WarmupPhase` ステートマシン** — スプラッシュ画面中の 5 段階 GPU パイプラインウォームアップ: `NotStarted` → `RendererCreated` → `SrgbMsaaDone` → `SrgbNoMsaaDone` → `Complete`。`ensure_pipelines` に明示 `msaa: bool` 引数を追加
- **`cpu_prep_model` / `gpu_finalize_model` 分離** — `build_gpu_model_inner` を CPU 専用 `cpu_prep_model`（頂点処理、`Send` 安全、BG スレッド実行）と GPU 専用 `gpu_finalize_model`（bind group/バッファ生成、メインスレッド）に分割。新型: `CpuPrepResult`・`CpuDrawPlan`・`PerMatGpuMeta`・`AuxTexRefs`。`PendingGpuBuild` に `cpu_prep_rx` チャネルを追加し 3 フェーズ非同期フローに拡張
- **`append_pkg_thumb_cache` 差分メソッド** — `pkg_textures[start_index..]` のみサムネイル生成。既存のサムネイル GPU テクスチャを保持したまま追記

### リファクタリング

- **`spawn_bg_task` 共通ヘルパー** — 4 つの `spawn_bg_*` 関数から共通ボイラープレート（キャンセル・mpsc チャネル・request_id・スレッド起動）を `spawn_bg_task` ヘルパーに集約。各呼び出し元は `CpuParseInput` と `fallback_kind` の構築のみ担当
- **`process_pending_tasks` の `poll_*` 分割** — 約 450 行のモノリシックメソッドを `poll_file_dialog`・`poll_dispatch_and_bg_load`・`poll_deferred_loads`・`poll_gpu_build`・`poll_export_tasks`・`poll_overlay_tasks`・`poll_convert_bg` に分割。`poll_receiver` ヘルパーで `try_recv` 4 分岐パターン（7 箇所）の重複を解消
- **`IrMesh` 重量フィールドの Arc 化** — `vertices`・`indices`・`morph_targets` を `Vec<T>` → `Arc<Vec<T>>` に変更。clone が O(1) 参照カウント化。mutation は `vertices_mut()` / `indices_mut()` / `morph_targets_mut()` で `Arc::make_mut`（COW）を使用
- **`assign_texture_core` 共通メソッド** — `assign_texture_source_to_material`（ファイルパス）と `assign_texture_data_to_material`（バイト列）の重複ロジックを共通コアに統合。ファイルパス側で欠落していた `mmd_texture_bind_group = None` クリアを修正
- **Append ロールバックの型安全化** — append 操作のフィールド単位の手動保存/復元を `IrRollbackSnapshot`・`LoadedModelOwnership`・`AnimationSnapshot` 構造体に集約
- **`TmpDirGuard` RAII クリーンアップ** — `convert_ir_to_pmx_with_cancel` 内の 5 箇所の `.inspect_err(|_| cleanup())?` パターンを Drop ベースのガードに置換。成功パスで `disarm()` によりクリーンアップを抑制
- **`MaterialBuildFlags` 構造体** — 4 つの並行スライス引数（`smooth_per_mat`・`clear_per_mat`・`normal_map_per_mat`・`emissive_per_mat`）を `build_gpu_model` / `cpu_prep_model` / `PendingGpuBuild` 横断で 1 構造体に集約
- **`write_model_opt_cancel`** — `PmxWriter` がセクション間（vertices・faces・textures・materials・bones・morphs）で協調キャンセルに対応。`write_pmx_and_stats` がキャンセルフラグを伝播

## v0.2.38

### パフォーマンス

- **Prefab テクスチャ解決インデックス** — `UnityPackageIndex` 構築時に `prefab_by_fbx_guid` 逆引きマップと `prefab_cache` を事前構築。`resolve_prefab_textures` が FBX ごとに全 `.prefab` エントリをフルスキャン（O(P×F)）していた処理を O(1) HashMap ルックアップに置換
- **Variant 解決キャッシュ** — `resolve_variant_multi` の結果を `variant_cache` にキャッシュ。`resolve_variant_multi_inner` の再帰呼び出しで Prefab YAML を毎回再パースしていた処理を `prefab_cache` 参照に置換
- **TextureData Arc 共有** — `TextureData::Encoded` の内部型を `Vec<u8>` → `Arc<[u8]>` に変更。`.unitypackage` からのテクスチャデータを `to_vec()` フルコピーではなく `Arc::clone`（O(1)）で共有
- **マテリアル GUID 重複チェック** — `resolve_prefab_textures` / `resolve_variant_multi_inner` 内のマテリアル GUID ユニーク化を `Vec::contains()`（O(N²)）から `HashSet`（O(N)）に変更
- **Prefab パース並列化** — `build_prefab_fbx_map` で `rayon::par_iter` を使用し、インデックス構築時の全 Prefab YAML パースをマルチスレッド並列化

### 改善

- **一括読み込み進捗トースト** — 複数モデル一括読み込み時にモデルごとの進捗をトースト通知で表示（例: 「読み込み中 (2/5)：model.fbx」）。進捗情報を `PendingPkgModelLoad.batch_progress` に保持し、最終件でも `PendingMultiLoad` 破棄後に表示が消えないよう対応

## v0.2.37

### バグ修正

- **追加読み込み Prefab のリロード時インデックス不整合修正** — 追加読み込みした `.unitypackage` Prefab モデルに対して A スタンス / T スタンス変換を行うと `Prefab parse failed` エラーが発生するバグを修正。`reload_append_unitypackage` が `extract_all_assets()` と `build_unity_package_index()` を別々に呼び出しており、HashMap のイテレーション順序が非決定的なためエントリ配列の順序が一致しなかった。1回目の配列でパス名検索したインデックスが、2回目の配列では無関係なファイル（例: `.shader`）を指す可能性があった。`try_load_unitypackage_for_append` と同じパターンで `UnityPackageIndex` を1回だけ構築し、そこから `ExtractedAsset` を導出するよう修正

## v0.2.36

### 改善

- **lilToon エミッション Screen ブレンド減衰** — lilToon の `_EmissionBlend: 1`（Screen モード）の材質で `emissive_factor` を 0.5 倍に減衰し、Screen 合成（`base + emission*(1-base)`）を近似する。Screen は加算合成より常に暗くなるが、従来は純粋な加算合成で描画されていたため、Refrain_V2 等の材質で過剰な明るさと Bloom の白飛びが発生していた
- **Prefab `_UseEmission` 優先判定** — Prefab パス（`unitypackage.rs`）のエミッション有効判定で、lilToon の `_UseEmission` float を Standard シェーダーの `_Emission` float より優先して参照するように変更。従来は `_UseEmission: 0` の材質でも `_EmissionMap` テクスチャの存在によるフォールバックで誤検出される可能性があった
- **UI 用語の統一** — 材質のエミッシブトグルの表記を `Bloom/Emissive` / `Bloom（グロー）` の混在から用途別に統一: ポストプロセスエフェクト → `Bloom`、材質単位の発光 → `エミッシブ`。内部フィールド名も `MaterialDisplayState::bloom` → `emissive` にリネーム

## v0.2.35

### 改善

- **シングルインスタンス IPC 長パス対応** — Named Pipe リスナーで `ERROR_MORE_DATA` 発生時にループ読み取りを行い、任意の長さのファイルパス（UTF-8 の深いネスト日本語ディレクトリ名を含む）に対応。読み取り失敗時の部分データは破棄し、不正なパスの処理を防止。バッファサイズを 32KB → 64KB に拡大。パイプハンドルを RAII ラッパー（`WinHandle` + `Drop` 実装）で管理し、早期リターンやパニック時のハンドルリークを防止
- **LogBuffer パフォーマンス改善** — インメモリログバッファを `Vec<u8>` から `VecDeque<u8>` に変更。上限超過時の先頭切り詰め（`drain(..excess)`）が O(N) memmove → O(1) 償却に改善
- **aux_files クローン削減** — `load_model_from_path_core` の PMX/PMD ロードで `aux_files` を1回だけ取得し、モデル解析（参照渡し）とソース構築（ムーブ）の両方で使い回し。ロードごとの不要な `HashMap` クローンを排除
- **reload_from_source クローン削減** — 中間変数 `source_clone` を廃止し、`&ReloadableSource` パラメータを直接 match。`finish_load` の直前でのみ1回クローンすることで、リロードごとのディープクローン回数を半減
- **パイプライン取得のパニック診断改善** — `gpu.rs` の `pipelines()` メソッドで `unwrap()` を説明付き `expect()` に変更し、`ensure_pipelines` 呼び忘れ時の診断を容易化

### ドキュメント

- **商標・権利帰属セクション追加** — VRM（VRM Consortium）、FBX（Autodesk）、glTF（Khronos Group）、DirectX（Microsoft）、PSD（Adobe）、PMX/PMD、OBJ、STL の商標帰属を追記。MToon、UTS2、lilToon、Poiyomi のシェーダー技術クレジットとライセンス情報を追記
- **依存ライブラリライセンス表更新** — 漏れていた5クレート（`toml`、`dunce`、`tempfile`、`encase`、`env_logger`）を依存一覧・ライセンス表の両方に追加。`encoding_rs` のリポジトリ URL を修正（`nickel-org` → `hsivonen`）。`dunce` のリポジトリ URL を修正（GitHub → GitLab `kornelski/dunce`）

## v0.2.34

### 新機能

- **Prefab の追加読み込み（append）対応** — 既にモデルが読み込まれている状態で、Prefab を追加読み込みできるようになった。`append_from_pkg` 内で Prefab の GUID 参照チェーンを解決し、参照先 FBX をすべて展開、`embed_textures_with_prefab` でテクスチャマッピングを適用後、既存シーンにマージする。従来は Prefab を append モードで選択するとエラーになっていた
- **複数モデル一括読み込み** — `.unitypackage` のモデル選択ダイアログにチェックボックスを追加。複数モデルを選択して「まとめて読み込み (N)」ボタンで一括ロードが可能。1つ目を通常ロード、2つ目以降を `PendingMultiLoad` キューで順次 append する。従来の単一クリックによる即時ロードも維持

### 改善

- **ゼロコピーアセット共有** — `take_fbx_and_textures` / `take_vrm` が `Vec<ExtractedAsset>` を消費する代わりに `&[ExtractedAsset]` を借用する方式に変更。`PendingPkgModelLoad.assets` を `Arc<Vec<ExtractedAsset>>` にし、一括ロード時のモデルごとのアセット複製を排除
- **一括ロードの失敗時中断** — バッチ内のいずれかのモデルロードが失敗した場合、または FBX 読み込みモード選択ダイアログがキャンセルされた場合、残りのキュー（`PendingMultiLoad`）を即座に破棄して不正な append を防止

## v0.2.33

### 新機能

- **lilToon / Poiyomi シェーダー検出（Phase 3）** — `ShaderFamily` enum に `LilToon` / `Poiyomi` バリアントを追加し、VRM 0.0 の `materialProperties.shader` フィールドから自動検出。検出パラメータを `MtoonParams` に近似変換: shade color / shadow border・blur / アウトライン（幅・色・マスク）/ リムライト / MatCap / エミッシブ / 法線マップ / アルファモード / カリング。2nd shadow color は PMX ambient にマッピング。PMX 変換では抽出時に設定した ambient/specular をそのまま使用（UTS2 と同パターン）。検出はシェーダー名マッチング + プロパティのみのフォールバック（lilToon: `_lilToonVersion`、Poiyomi: `_EnableShadow` + `_Shadow1stColor`）

### 改善

- **ログのシェーダー種別表示** — 材質一覧ログで `mtoon=true/false` の代わりに `shader=MToon` / `shader=lilToon` / `shader=Poiyomi` / `shader=UTS2` / `shader=-` を表示するよう変更。`ShaderFamily` enum に `Display` トレイトを実装
- **シェーダー対応ドキュメント** — 使い方ドキュメント（日英両方）に「シェーダー対応状況」セクションを追加。シェーダー検出条件表とシェーダー別再現度表を記載

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

