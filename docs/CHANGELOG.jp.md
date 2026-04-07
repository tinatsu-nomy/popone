<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [更新履歴](#%E6%9B%B4%E6%96%B0%E5%B1%A5%E6%AD%B4)
  - [v0.2.34](#v0234)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD)
    - [改善](#%E6%94%B9%E5%96%84)
  - [v0.2.33](#v0233)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-1)
    - [改善](#%E6%94%B9%E5%96%84-1)
  - [v0.2.32](#v0232)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-2)
    - [コード品質・パフォーマンス改善](#%E3%82%B3%E3%83%BC%E3%83%89%E5%93%81%E8%B3%AA%E3%83%BB%E3%83%91%E3%83%95%E3%82%A9%E3%83%BC%E3%83%9E%E3%83%B3%E3%82%B9%E6%94%B9%E5%96%84)
  - [v0.2.31](#v0231)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-3)
    - [改善](#%E6%94%B9%E5%96%84-2)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# 更新履歴

[English](CHANGELOG.md)

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

