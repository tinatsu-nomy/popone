<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [更新履歴](#%E6%9B%B4%E6%96%B0%E5%B1%A5%E6%AD%B4)
  - [v0.5.5（2026-04-13）](#v0552026-04-13)
    - [新機能 (Phase 1)](#%E6%96%B0%E6%A9%9F%E8%83%BD-phase-1)
    - [新機能 (Phase 2)](#%E6%96%B0%E6%A9%9F%E8%83%BD-phase-2)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85)
    - [スコープ注記](#%E3%82%B9%E3%82%B3%E3%83%BC%E3%83%97%E6%B3%A8%E8%A8%98)
    - [バグ修正（リリース前レビュー対応）](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3%E3%83%AA%E3%83%AA%E3%83%BC%E3%82%B9%E5%89%8D%E3%83%AC%E3%83%93%E3%83%A5%E3%83%BC%E5%AF%BE%E5%BF%9C)
    - [テスト](#%E3%83%86%E3%82%B9%E3%83%88)
  - [v0.5.4（2026-04-13）](#v0542026-04-13)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85-1)
    - [バグ修正（リリース前レビュー対応）](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3%E3%83%AA%E3%83%AA%E3%83%BC%E3%82%B9%E5%89%8D%E3%83%AC%E3%83%93%E3%83%A5%E3%83%BC%E5%AF%BE%E5%BF%9C-1)
    - [テスト](#%E3%83%86%E3%82%B9%E3%83%88-1)
  - [v0.5.3（2026-04-13）](#v0532026-04-13)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-1)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85-2)
  - [v0.5.2（2026-04-13）](#v0522026-04-13)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-2)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85-3)
    - [バグ修正（リリース前レビュー対応）](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3%E3%83%AA%E3%83%AA%E3%83%BC%E3%82%B9%E5%89%8D%E3%83%AC%E3%83%93%E3%83%A5%E3%83%BC%E5%AF%BE%E5%BF%9C-2)
  - [v0.5.1（2026-04-13）](#v0512026-04-13)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-3)
    - [パフォーマンス](#%E3%83%91%E3%83%95%E3%82%A9%E3%83%BC%E3%83%9E%E3%83%B3%E3%82%B9)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85-4)
    - [バグ修正（リリース前レビュー対応）](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3%E3%83%AA%E3%83%AA%E3%83%BC%E3%82%B9%E5%89%8D%E3%83%AC%E3%83%93%E3%83%A5%E3%83%BC%E5%AF%BE%E5%BF%9C-3)
    - [テスト](#%E3%83%86%E3%82%B9%E3%83%88-2)
    - [v0.6.0 に延期](#v060-%E3%81%AB%E5%BB%B6%E6%9C%9F)
  - [v0.5.0（2026-04-13）](#v0502026-04-13)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-4)
    - [挙動の変更](#%E6%8C%99%E5%8B%95%E3%81%AE%E5%A4%89%E6%9B%B4)
    - [テスト](#%E3%83%86%E3%82%B9%E3%83%88-3)
  - [v0.4.0（2026-04-11）](#v0402026-04-11)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-5)
    - [挙動の変更](#%E6%8C%99%E5%8B%95%E3%81%AE%E5%A4%89%E6%9B%B4-1)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85-5)
  - [v0.3.0（2026-04-11）](#v0302026-04-11)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# 更新履歴

[English](CHANGELOG.md)

## v0.5.5（2026-04-13）

**材質編集パネルから呼び出す頂点単位 UV 編集ウィンドウ**を追加。v0.5.4 は材質単位の UV 変形（offset / scale / rotation）を提供したが、v0.5.5 はその下のレイヤーに踏み込み、**Phase 1**（単一頂点エディタ＋永続化＋reload 保持）と **Phase 2**（テクスチャ背景・矩形選択・ズーム/パン・回転/スケール・undo/redo・Ctrl+A）を同梱する。Phase 3（UV1 / モーフ UV / 複数ウィンドウ）は v0.5.7 以降に延期。

### 新機能 (Phase 1)

- **UV 編集ウィンドウ** — 材質編集パネルのヘッダに「UV 編集」ボタンを追加。クリックでフローティング `egui::Window`（`Id::new("uv_edit_window")` で 1 インスタンス固定）が開き、タイトルにアクティブ材質名を表示。ウィンドウ内に最大 260×260 px の正方形キャンバスでアクティブ材質の三角形ワイヤーフレームを UV 空間（**v=0 を上端 / v=1 を下端**）で表示。`convert/uvmap.rs` の PSD 出力と同じ向きなので両者を直接並べて比較できる
- **頂点ピック & ドラッグ** — UV 頂点から 12 px 以内をクリックすると選択（黄色）、ドラッグで UV 空間上を平行移動。編集結果は `IrMesh.vertices_mut()[*].uv` に直接書き込まれるため、再エクスポート（PMX 出力）に即時反映される
- **材質フィルタ** — ウィンドウ内 ComboBox でキャンバスに表示する材質を切り替え。材質編集パネルの「UV 編集」ボタンからの起動時は現在編集中の材質を自動でアクティブにセット
- **頂点 UV の永続化** — `TextureHistoryFile` に `vertex_uv_overrides: HashMap<path, Vec<VertexUvOverrideEntry>>` を追加（JSON バージョンを v3 に昇格）。「履歴を保存」で頂点単位 UV 差分をテクスチャ・パラメータ差分と並んで書き出し、「履歴呼出」で復元して GPU vertex buffer を再同期
- **mouse-up 時のみ GPU 同期** — `GpuModel::sync_uvs_from_ir` はドラッグ終了時にのみ vertex buffer を全転送。ドラッグ中は CPU 側の `IrVertex.uv` 更新のみでフレームレートに影響しない

### 新機能 (Phase 2)

- **テクスチャ背景 (2-1)** — アクティブ材質の BaseColor テクスチャを `register_native_texture` + `painter.image` でキャンバス背景に描画。UV [0,1] 領域のみを対象とする。1 エントリキャッシュ（`ViewerApp.uv_edit_bg_tex`）で材質変更時のみ張り替え、`finish_load_with_gpu` でモデル切替時に `free_texture` して GPU リークを回避。PMX/PMD 材質で `base_color_tex_info` が `None` の場合は `mat.texture_index` にフォールバック
- **矩形選択 + 一括平行移動 (2-2)** — 頂点から遠い位置でドラッグ開始すると矩形選択（既存選択をクリア、矩形内頂点を毎フレーム再計算して selected に反映）。頂点近傍から開始すると Move モードに入り、選択頂点全体を平行移動。`UvDragMode { None, Move, Rect }` でモード分岐、`drag_start_uvs` は HashMap なので 1 頂点でも N 頂点でも同一コードパス
- **ズーム / パン / スナップ (2-3)** — ホイールでカーソル中心にズーム（0.1×〜32×、対数スケール `* 0.002.exp()`）。中ボタンドラッグでパン（ズーム倍率を考慮）。Shift+ドラッグで平行移動を 1/16（= 0.0625）グリッドにスナップ。`uv_to_canvas` / `canvas_to_uv` に `view_offset` / `view_zoom` を引数追加し、ピック/描画/ドラッグ/矩形計算すべてが自動でビュー変換に追従。「表示リセット」ボタンで zoom=1.0 / offset=[0,0] に復帰
- **回転 / スケール (2-4)** — Alt+ドラッグで選択 bbox 中心ピボットの回転（`atan2` で角度差 → `sin_cos` で回転行列）。Ctrl+ドラッグで同ピボットのスケール（距離比）。Move ドラッグ中はピボットに十字マーカーを描画して視覚フィードバック。Ctrl と Alt が同時押下の場合は Ctrl 優先
- **undo / redo (2-5)** — `UvUndoEntry { before, after }` を drag_stopped Move ごとに 1 コマンド記録。`Ctrl+Z` で undo、`Ctrl+Y` / `Ctrl+Shift+Z` で redo。「⟲ 元に戻す」「⟳ やり直す」ボタンが GUI から同等操作を提供。undo スタックは 50 エントリ上限（FIFO）。新編集で redo スタックは自動クリア（標準セマンティクス）。`wants_keyboard_input()` ガードで TextEdit とキー衝突しない
- **全選択 (Ctrl+A)** — アクティブ材質の全頂点を `selected` に追加（既存選択は保持）。「全選択」ボタンが GUI から同等操作を提供

### 内部実装

- 新規 `src/viewer/app/uv_edit.rs` に `UvEditState` を実装し、フェーズを追うごとに拡張:
  - Phase 1: `overrides`, `selected`, `active_material`, `dragging`, `pending_restore`
  - Phase 2-2: `UvDragMode` enum, `drag_mode`, `drag_start_uvs`, `drag_press_uv`
  - Phase 2-3: `view_offset`, `view_zoom` (`view_zoom = 1.0` のため `Default` を手書き), `reset_view()`
  - Phase 2-4: `drag_pivot`
  - Phase 2-5: `UvUndoEntry`, `undo_stack`, `redo_stack`, `push_undo`, `apply_undo`, `apply_redo`, 定数 `UV_UNDO_MAX = 50`
  - レビュー 05 対応: `pristine_uvs`, `record_pristine`, `matches_pristine`（遅延記録の per-vertex pristine。undo が pristine に戻した頂点を overrides から自動除外）
- `ViewerApp` に `uv_edit_window_open: bool` と `uv_edit_bg_tex: Option<(usize, egui::TextureId)>` を追加。`show_uv_edit_window` は `update()` の `show_material_editor_window` 直後に呼び出し、両フィールドとも `finish_load_with_gpu` でクリア / free される
- `persistence.rs` に `VertexUvOverrideEntry { mesh_index, vertex_index, uv: [f32; 2] }` を追加（フラット配列 JSON ~30 byte/頂点）。`TextureHistoryFile` は `#[serde(default)]` により v0.5.4 以前の `popone_history.json` もそのまま読める後方互換
- `GpuModel::sync_uvs_from_ir` は IR メッシュを走査し、既存の `global_to_gpu` マップで GPU 頂点Index を逆引きして `base_vertices[*].uv` を書き換え、`queue.write_buffer` で転送後にモーフキャッシュを無効化する
- `uv_to_canvas` / `canvas_to_uv` に `view_offset: [f32; 2]` / `view_zoom: f32` 引数を追加。描画・ピック・ドラッグ・矩形計算がすべてこの 2 関数経由なので、Phase 2-3 のパン/ズームが 1 箇所の変更で全インタラクションに波及
- ドラッグ処理は `start_uv + (cursor_uv - press_uv)` 方式（および回転/スケール の類似式）で egui バージョン間の `drag_delta()` 仕様に依存しない

### スコープ注記

- **UV0 のみ対応**。UV1（`IrMesh.uvs1`）・モーフ UV・複数 UV セット切替は Phase 3（v0.5.7 以降）で対応予定
- **単一ウィンドウインスタンス**。複数材質の横並び編集は Phase 3
- **「編集をすべてクリア」は破壊的**。overrides / selected / undo スタック / redo スタック / pristine_uvs を全て破棄する。編集前の UV に完全に戻すにはリロードが必要（モデル全体の pristine を保持しないため）

### バグ修正（リリース前レビュー対応）

- **review_result_01 [P1] reload で頂点 UV 編集が消える問題を修正** — `finish_load_with_gpu` が無条件で `self.uv_edit.reset()` を呼んでいたため、A スタンス / T スタンス変換や `reload_current()` を挟むと未保存の UV 編集が失われていた。`ReloadSnapshot` に `uv_edit_overrides` / `uv_edit_active_material` / `uv_edit_window_open` を退避し、`restore_snapshot_on_success` で `apply_to_ir` による IR 反映と `sync_uvs_from_ir` による GPU vertex buffer 再同期を行う。`restore_snapshot_on_failure` でも overrides を書き戻し、失敗時のメモリ状態の整合を保つ
- **review_result_02 [P1] ドラッグ移動が毎フレーム過加算される問題を修正** — 旧実装は `dragged()` フレーム毎に `response.drag_delta()` を現在 UV へ加算していたが、egui バージョンによっては累積量を返すケースがあり、フレーム数に比例して UV が飛んでいた。新実装は `drag_started()` で開始 UV とカーソル UV を記録し、`new_uv = start_uv + (cursor_uv - press_uv)` で再計算する方式に変更。`canvas_to_uv` ヘルパーと `UvEditState::{drag_start_uvs, drag_press_uv}` を追加
- **実機確認 [P1] UV キャンバスと PSD 出力の Y 方向が逆転** — 初期 Phase 1 実装は Blender/Maya 慣例で v=0 を下端に配置していたが、`convert/uvmap.rs` は UV v をそのまま画像 Y にラスタライズ（v=0 が上端）していた。`uv_to_canvas` / `canvas_to_uv` を両方 Y 非反転に変更し、PSD 出力と同じ向きに統一
- **review_result_03 [P2]「編集をすべてクリア」が undo/redo 履歴を残していた** — 旧実装は `overrides` / `selected` しかクリアせず、直後に `Ctrl+Z` を押すと破棄したはずの UV 編集が復活し、UI ラベルの契約に反していた。`undo_stack` / `redo_stack` も同時破棄し、ホバーテキストに「undo/redo 履歴を破棄」を明記
- **review_result_05 [P2] undo 後も元 UV が override として残っていた** — `apply_undo` / `apply_redo` が常に `overrides.insert(k, v)` していたため、頂点を元位置に戻しても overrides にエントリが残り、UI 件数と `to_entries()` 経由の保存結果が実状態とずれていた。遅延記録の `pristine_uvs`（drag_started で `or_insert`）を導入し、適用後の UV が pristine と一致するなら `overrides.remove` するように変更。メモリは「一度でも編集された頂点」分のみで、Phase 1 で避けた全頂点 pristine とは別物の軽量設計
- **review_result_06 [P2]「編集をすべてクリア」が pristine_uvs を残していた** — レビュー 05 対応後、クリアボタンは `pristine_uvs` も破棄する必要が判明。残したままだと次の編集セッションで古い pristine を `or_insert` で再利用し、undo で「未編集状態」に戻れなくなる不具合があった

### テスト

- 既存 179 テスト全通過。UV 編集ロジックは UI 主体で、下流のデータフロー（IR → GpuModel → PMX writer）は既存のラウンドトリップテストで既にカバー済み

## v0.5.4（2026-04-13）

材質編集パネルにスロット毎の UV 変形（offset / scale / rotation）編集 UI を追加。KHR_texture_transform を持つ 9 スロット（BaseColor / Emissive / Normal / Shade / ShadingShift / RimMultiply / OutlineWidth / Matcap / UvAnimMask）が対象。

### 新機能

- **スロット毎 UV 変形編集 UI** — 各テクスチャスロットのサムネイル直下に UV 変形コントロール（offset X/Y、scale X/Y、rotation°、リセットボタン ⟲）を追加。テクスチャが割り当てられているスロットにのみ表示され、値は `IrTextureInfo.offset / scale / rotation` に即時反映されて GPU uniform（`base_uv / shade_uv / ...` の 9 ペア）へ送信される。rotation は度入力・ラジアン保存
- **UV 変形の永続化** — `MaterialParamOverride` に 9 スロット分の `TextureUvOverride { offset, scale, rotation }` フィールドを追加。`popone_history.json` にモデルパス単位で保存され、reload / A スタンス変換 / 再起動後に自動復元される。v0.5.3 以前の JSON は `#[serde(default)]` によりフィールド欠落でそのまま読込可能
- **Expression 駆動 UV アニメとの共存** — v0.5.1 で配線済の `IrTextureTransformBind` とは独立経路で動作し、静的 UV override → Expression 加算の順で両立

### 内部実装

- `TextureUvOverride` 型を新規追加（`offset: Option<[f32;2]>`, `scale: Option<[f32;2]>`, `rotation: Option<f32>`）。全フィールド `Option` のため部分保存が可能で、serde でのサイズも最小化
- `apply_to` は **既存 `IrTextureInfo` が Some の場合のみ** UV を書き込む。未割当スロットに対しては no-op（勝手に `IrTextureInfo::from_index(0)` を生成して誤テクスチャを差し込まない）
- MToon スロット UV は `mat.mtoon.as_mut()` 経由で書き込み、`mat.mtoon_mut()` は呼ばない（非 MToon 材質に default mtoon を自動挿入しないための防御）
- `diff_from` は `enable_mtoon == Some(false)` の場合、MToon スロット UV 6 種も含めて diff を skip（round-trip 一貫性）
- UI ヘルパー `uv_transform_widget` / `record_uv_override` を `ui.rs` に追加し 9 箇所で共用

### バグ修正（リリース前レビュー対応）

- **review_result_01 [P1] 新規割当スロットの UV 編集が履歴保存されない** — `TextureUvOverride::diff()` が `(Some, Some)` の場合しか差分を返さず、pristine が未割当で current が新規割当 + UV 編集済みのケースで履歴保存時に UV 差分が落ちていた。`(None, Some)` のときも default transform（offset=0 / scale=1 / rotation=0）との比較にフォールバックし、新規割当スロットの UV 編集を正しく保存するよう修正

### テスト

- UV round-trip: BaseColor / MToon スロット (shade) の diff → apply が等価（+2 件）
- 未割当スロットへの apply が no-op であること / mtoon 未初期化時に mtoon を生成しないこと（+2 件）
- MToon OFF 時に MToon スロット UV が diff に含まれないこと（+1 件）
- `TextureUvOverride::default()` の `is_empty` 動作 / `merge_from` の UV マージ（+2 件）
- 新規割当スロット UV の保存 / スロット解除時の UV 扱い（+2 件、review_result_01 [P1] 対応）
- 合計 +9 件（material_edit tests 計 19 件、全体 244 件パス）

## v0.5.3（2026-04-13）

材質編集 UI の刷新: フローティング Window からショートカットヒントバー直上の固定ドック型パネルへ変更。材質名編集、先頭ボタンのサムネイル表示、アイコン絵文字化、法線一括操作の ON/OFF ボタン化を一括適用。

### 新機能

- **材質名編集** — 材質編集パネル冒頭に TextEdit を追加し、材質名をその場で変更可能。`MaterialParamOverride.name: Option<String>` 経由で `material_overrides` に記録され、reload / A スタンス変換 / `popone_history.json` 保存すべてで復元される。変更後は `update_mat_cache()` でサイドパネルの材質リスト表示に即時反映
- **材質編集パネルの固定ドック化** — 旧フローティング `egui::Window` を `egui::TopBottomPanel::bottom("material_editor_panel")` に変更。ショートカットヒントバー直上に固定表示し、上辺ドラッグで伸縮、内部は `ScrollArea::vertical` でスクロール可能。右上の `[×]` ボタンで閉じる。[編] アイコンが OFF のときはパネル自体が出現せず、中央ビューポートが自動拡張する
- **材質行先頭ボタンのサムネイル化** — 旧 □/■ 文字インジケータを `ir_thumb_cache` の 14×14px テクスチャサムネ `ImageButton` に置換。`ui.scope` 内でのみ `spacing.button_padding = (1,1)` + `stroke = 0.5` を適用するコンパクト枠プリセットで、他 UI ボタンに影響を与えずに絵文字アイコン列と視覚的に揃える。サムネ取得不可時は従来の □/■ にフォールバック（割当済だがサムネ未生成時は ■）
- **アイコン絵文字化** — 材質行と材質グループヘッダの `[S][C][N][B][編]` ラベルを `✨🗑🗺💡✏` に置換。`ICON_SMOOTH / ICON_CLEAR_NORMAL / ICON_NORMAL_MAP / ICON_EMISSIVE / ICON_EDIT` を `ui.rs` 冒頭の定数として 1 箇所に集約
- **法線一括操作の ON/OFF ボタン化** — 旧「法線平滑化（一括）」「カスタム法線クリア（一括）」チェックボックスを `ラベル + [on] + [off]` の小ボタン列に置換。on 操作時の法線マップ付き材質スキップ仕様は維持

### 内部実装

- `MaterialParamOverride` に `name: Option<String>` フィールドを追加。`String` は `Copy` でないため `merge_from` / `diff_from` / `apply_to` では既存の `merge!` / `diff_field!` マクロ対象外として個別 `clone()` で処理
- `update_mat_cache` の可視性を `pub(super)` → `pub(in crate::viewer)` に拡張し、`ui.rs` から材質名キャッシュを再構築できるようにした
- `show_side_panel` 先頭で `app.sync_ir_thumb_cache()` を呼ぶよう変更（length 比較で早期 return するため同期済なら無コスト）
- 材質編集パネルの呼び出し位置を `apply_pending_material_rebuilds()` 前から `shortcut_hints` パネル追加の直後に移動し、egui の下部パネル積み上げ順「最下=status_bar / 中=shortcut_hints / 上=編集パネル」を確保

## v0.5.2（2026-04-13）

材質編集ドロワーの各パラメタセクションにテクスチャサムネイルを統合。テクスチャと関連パラメタが 1 箇所で見られるようになった。

### 新機能

- **テクスチャサムネイルのセクション統合** — 旧「テクスチャスロット」集約セクションを解体し、各材質セクションの先頭にサムネイル + 割当UI を配置した:
  - **基本**: BaseColor
  - **影 (Shade)**: Shade / ShadingShift
  - **アウトライン**: OutlineWidth
  - **リム**: RimMultiply
  - **MatCap**: Matcap
  - **UV アニメ**: UvAnimMask
  - **エミッシブ / 法線**: Emissive / Normal
  - **MMD テクスチャ (Sphere / Toon)**: Sphere / Toon（MMD/PMX 固有のため別セクションとして残置）
- **サムネイルがそのままボタン** — 32px のサムネイル画像自体が `ImageButton` になり、クリックでファイルダイアログが開く（従来のテキストボタンは廃止）。ホバーでファイル名をツールチップ表示。
- **未割当スロットは X アイコン** — 割当のないスロットには `×` 印のプレースホルダボタンを描画。色はテーマの `widgets.inactive` に連動。クリックで新規割当ダイアログが開く。
- **行末の `×` はスロットリセット** — 既存挙動を踏襲し、割当済みスロットのみに表示（小さな `×` ボタン）。

### 内部実装

- `TextureState` に `ir_thumb_cache: Vec<Option<egui::TextureId>>` を追加し、`loaded.ir.textures` と並列のサムネイル TextureId キャッシュを保持。既存の `pkg_thumb_cache`（UnityPackage 内テクスチャ用）と同じ 64px サムネイルパイプラインを流用。
- `rebuild_ir_thumb_cache` / `append_ir_thumb_cache` / `clear_ir_thumb_cache` / `sync_ir_thumb_cache` の 4 メソッドを追加。`sync` は `loaded.ir.textures.len()` と cache 長を比較し、増加時のみ append、減少時は rebuild、`loaded` が無い時は clear する差分更新を行う。
- `assign_texture_core` の新規テクスチャ push 経路で直接 `build_ir_thumb_entry` を呼び、`&mut self` 再取得による borrow 衝突を回避しつつサムネイルを同期追加。
- `show_material_editor_window` の先頭で `sync_ir_thumb_cache` を呼び、モデル切替・BG ロード完了など外部経路で `ir.textures` 長が変化しても UI 表示が追従する設計。
- 共通 `texture_slot_widget()` ヘルパー関数を追加。各セクションから呼び出し、`(assign_clicked, reset_clicked)` の bool ペアを返して呼び出し側で `pending_tex_request` / `pending_tex_clear` を設定する借用境界設計。

### バグ修正（リリース前レビュー対応）

- **[review_01 P1] モデル切替後に前モデルのサムネイルが残る問題を修正** — `finish_load_with_gpu` / `cancel_gpu_build` / `cancel_bg_index_load` で `clear_ir_thumb_cache()` を呼び出すようにした。前モデルと新モデルのテクスチャ数が一致した場合、`sync_ir_thumb_cache()` が長さ比較だけで early return するため、前モデルの `TextureId` がそのまま再利用されて別モデルのサムネイルが誤って表示される問題があった。
- **[review_01 P2] PSD→PNG 変換後にサムネイルが更新されない問題を修正** — `poll_pending_psd_conversions()` で PNG 差し替え完了時に該当 `tex_idx` の `TextureId` を再生成するようにした。`sync_ir_thumb_cache()` は長さが変わらないため再構築されず、PSD デコード失敗で初期サムネイルが `None` だったケースでは、PNG 変換完了後も永続的に空欄のまま残る問題があった。
- **[review_02 P1] 材質編集ウィンドウ未表示時のテクスチャ追加で index がずれる問題を修正** — `assign_texture_core()` / `apply_tex_preview()` の新規テクスチャ push 経路で、単純な `cache.push()` ではなく「不足分を末尾から一括 append」するロジックに変更。`ir_thumb_cache` は材質編集ウィンドウを一度開くまで長さ 0 のままのため、従来の push だけでは新規サムネイルが `cache[0]` に入り、既存スロットの表示と index がずれる問題があった（モデル読込直後にメイン UI から BaseColor を差し替えるだけで全スロットの表示が壊れる）。

## v0.5.1（2026-04-13）

VRM 1.0 Expression 材質バインド再生、補助テクスチャスロットの永続化、材質編集ドロワー UX 改善。

### 新機能

- **Expression 材質バインド（VRM 1.0）** — VRM 1.0 Expression の `materialColorBinds` / `textureTransformBinds` をビューアが再生時に正しく処理するようになった。6 種のカラー対象（`color` / `emissionColor` / `shadeColor` / `matcapColor` / `rimColor` / `outlineColor`）と UV スケール/オフセットが、VRM 1.0 仕様のアルゴリズム `finalValue = baseValue + Σ((targetValue − baseValue) × weight)` に従って複数 Expression 間で加算合成される。ベース値はロード時にキャプチャされ、材質エディタで編集すると更新されるため、エディタ編集後の値が新しいベースとして Expression のブレンド基準になる。
- **Sphere / Toon テクスチャスロット編集** — 材質編集ドロワーの「テクスチャスロット」セクションに MMD 固有の `Sphere` / `Toon` スロットを追加。既存の 8 補助スロットと同じ UI パターン（ファイルダイアログボタン + `×` リセットボタン）。
- **補助テクスチャスロットの永続化** — BaseColor 以外の全 10 スロット（Emissive / Normal / Shade / ShadingShift / RimMultiply / OutlineWidth / Matcap / UvAnimMask / Sphere / Toon）のテクスチャ割当が `popone_history.json` に保存されるようになった。`TextureHistoryEntry` に `slot` フィールドを追加。以前は BaseColor のみ永続化され、補助スロットは再起動で消失していた。新しい `slot` フィールドは `#[serde(default)]` により、v0.5.0 の履歴ファイルは `BaseColor` として解釈される（後方互換）。逆に v0.5.1 で保存した履歴を v0.5.0 で読み込んでも unknown field は無視されるため、両方向互換性を確保。
- **材質編集ドロワーのダーティインジケータ** — 編集中の材質にパラメータ編集差分・BaseColor テクスチャ割当・補助スロットテクスチャ割当のいずれかがあるとき、ウインドウタイトルの末尾に `*` を表示する。これにより「触った材質」を一目で識別できる。
- **材質パラメータのコピー / ペースト** — 材質編集ドロワーのツールバー行に「コピー」「ペースト」ボタンを追加。コピーは `diff_from(pristine, current)` の結果をセッションローカルのクリップボードに保存。ペーストは編集中の材質に通常の dirty 追跡フローで適用する。テクスチャ割当はパス依存を避けるため意図的に除外しており、カラー/スカラー値のみが材質間で転送される。
- **PMX 非対応バッジの視覚強化** — リム / MatCap / UV アニメの CollapsingHeader は以前はセクションタイトルに `(PMX非対応)` が plain text で埋め込まれていた。タイトルはクリーンにし、各セクション本文の先頭に色付きの `⚠ PMX 非対応` バッジを配置。ホバーツールチップに「この項目は PMX 出力では反映されません。MME (.fx) 出力やビューアプレビューでは反映されます。」を表示。

### パフォーマンス

- **DrawCall uniform バッファ最適化** — `DrawCall` が `UNIFORM | COPY_DST` 属性の永続的な `wgpu::Buffer`（`material_buf`）を保持するようになった。`create_material_bind_group` を `serialize_material_uniform` / `create_material_buffer_and_bind_group` / `write_material_buffer` の 3 関数に分割。既存の bind group 再生成経路では uniform-only 更新に `queue.write_buffer` を使い、材質編集や Expression フレームごとの bind group 全再生成を回避する。これが GPU リソースチャーンなしで per-frame Expression 材質反映を実現する基盤となる。

### 内部実装

- `IrMorphKind` に `Material { color_binds, uv_binds }` バリアントを追加（既存の `Vertex` / `Group` と並列）。VRM Expression が頂点モーフと材質バインドの両方を持つ場合は同名の 2 つの IrMorph として発行し、既存の名前ベース `morph_weights` マッピングで両方に同一ウェイトが適用される（Compound バリアント不要）。
- `intermediate::types` に `MaterialColorBindType`、`IrMaterialColorBind`、`IrTextureTransformBind` の新型を追加。`MaterialColorBindType::from_vrm_str` で VRM 1.0 の `type` 文字列を enum にパース。
- `GpuMorphEntry::Material` バリアント + `GpuModel` に `MaterialBaseValues` フィールドを追加。アクティブな材質モーフを走査してカラー/UV 差分を材質ごとに蓄積し、dirty な材質のみ `Vec<Option<MaterialParams>>` を返す純関数 `accumulate_expression_materials()` を追加。
- `IrModel::merge()` が `IrMorphKind::Material` の `color_binds` / `uv_binds` 内 `material_index` を結合時にオフセット調整するようになった。

### バグ修正（リリース前レビュー対応）

Codex による 5 回のレビューラウンドで、新規の Expression 材質バインド経路と履歴呼出フロー / 既存の材質エディタとの統合問題が複数指摘され、全てリリース前に解消:

- **テクスチャ履歴呼出の順序修正** — 旧実装はテクスチャ復元 → pristine 復元の順で、2 ステップ目が補助スロットのテクスチャ参照（`emissive_texture` / `normal_texture` 等）を破壊していた。修正: pristine 復元を先に実行し、その後にテクスチャ復元 + param override を適用。pristine 復元と同時に `tex.assignments` / `tex.pkg_assignments` / `slot_texture_paths` もクリアして「呼出 = 保存時点の完全再現」を保証。
- **Expression ウェイト 0 復帰時の GPU 残留** — `accumulate_expression_materials` が `weight.abs() < 1e-6` の morph を完全にスキップしていたため、`1.0 → 0.0` のフレームでベース値が書き戻されず、最後に適用された色/UV が GPU 側に残留していた。修正: `GpuMorphEntry::Material` が参照する全材質を事前に dirty 扱いにすることで、weight=0 のとき accum がゼロ → `base + 0 = base` が書き戻される挙動に。
- **材質エディタ編集値が Expression のベースにならない** — `material_base_values` がモデルロード時に一度だけキャプチャされており、エディタ編集値が Expression の合成基準に反映されなかった。修正: `apply_pending_material_rebuilds` で dirty 時に `MaterialBaseValues::from_ir(mat)` を再キャプチャ。
- **手動モーフ中の編集で Expression 材質反映が消える** — 手動モーフスライダーで非ゼロ weight を保持しているときに材質エディタを触ると、Expression の材質反映が消失し、モーフを再度動かすまで戻らなかった。修正: `apply_pending_material_rebuilds` の末尾で、いずれかの morph weight が非ゼロなら Expression 材質反映を再実行。
- **full rebuild で BaseColor bind が再生成されない** — `rebuild_material_bind_groups` は `material_buf` / aux / MMD bind group のみを更新しており、標準パスの `texture_bind_group` は未更新だった。pristine 復元後に古い BaseColor テクスチャ bind が画面に残る不具合があった。修正: full rebuild パスで `texture_bind_group` も再生成。
- **PMX/PMD 材質の BaseColor bind 消失（上記修正の回帰）** — 最初の修正では `mat.base_color_tex_info` のみを参照していたため、PMX/PMD 材質は `texture_index` を持つが `base_color_tex_info` が None のため、full rebuild で texture_bind_group が None になり白テクスチャに後退していた。修正: 初回 DrawCall 構築と同じ情報源（`mat.texture_index` 優先、sampler は `base_color_tex_info.sampler` → デフォルトフォールバック）に揃えた。
- **可視材質 export で Material morph の material_index 再マップ漏れ** — `build_filtered_ir()` は mesh の `material_index` を `mat_remap` で詰め直していたが、`IrMorphKind::Material` の bind 内 `material_index` は clone のまま残留していた。修正: `color_binds` / `uv_binds` を `filter_map + mat_remap` で再マップし、除外材質を参照する bind は落とす。
- **空 Material morph が可視材質 export 後に残留** — 可視材質フィルタで bind 先が全除外された Material morph は、頂点依存しないという理由で常に生存扱いされ、機能しない「死んだ表情」として出力に残っていた。修正: Phase 3（morph_alive 判定）で Material morph の生存判定を「bind が 1 つでも `mat_remap` に残るか」に変更。Group モーフの収束ループがカスケードで孤立 Group も自然に除外。

### テスト

- ユニットテスト 235 件（v0.5.0 の 230 件から増加）。追加カバレッジ: `MaterialColorBindType::from_vrm_str` の 6 種の正当文字列 + 不明/空文字列フォールバック、`IrModel::merge()` における Material morph の material_index オフセット、`TextureHistoryEntry` の slot フィールド serde（後方互換デフォルト、明示指定、ラウンドトリップ）。

### v0.6.0 に延期

- 残りテクスチャスロット（RimMultiply / OutlineWidth / Matcap / UvAnimMask）の UV 変形編集 UI — 既存コードに UV 変形編集 UI 自体がまだ存在しないため、想定以上の実装スコープ。
- 材質編集ドロワー開時の D&D スロット選択ダイアログ。
- 自動割当のスロットヒントマッチ（`*_normal*` → Normal 等）。
- テクスチャスロットのサムネイルプレビュー。
- セクション折りたたみ状態のセッション間永続化。
- ユーザーカスタムプリセット保存 / 読込。
- sdPBR `.fx` 生成、MaskedMaterial 対応、MME `.fx` 読み込み。


## v0.5.0（2026-04-13）

MToon / lilToon の全パラメータを GUI で編集できる材質編集ドロワーと、MME（ray-mmd）向け `.fx` マテリアル生成機能を追加。

### 新機能

- **材質編集ドロワー** — 材質行の「編」ボタンから独立フローティングウインドウ（`egui::Window`）を開く。セクション構成: 基本（diffuse / alpha mode / baseColor テクスチャ）、影（shade_color / shading_toony / shading_shift ＋テクスチャ）、アウトライン（edge_color / 幅モード / outline width テクスチャ）、リム（パラメトリックリム / rim multiply テクスチャ）、MatCap、UV アニメ、エミッシブ / 法線、その他、MME 出力プレビュー。
- **MToon / lilToon 全パラメータ編集** — 25 個以上のカラー / スカラーと、補助テクスチャスロット（normal / emissive / shade / shadingShift / rim / outline / matcap / uvAnimMask）をすべて GUI 編集可能。編集値は標準描画パスと MMD 互換描画パスの両方に即時反映される。
- **スロット単位・材質単位のリセット** — 各スロットカードに `×` ボタンを設置してそのスロットだけを消去可能。材質単位の「初期値に戻す」はロード時点のスナップショットから復元する。
- **組み込みプリセット** — MToon 1.0 デフォルト / lilToon 標準 / PMX 互換 の 3 種類を同梱。
- **材質編集の永続化** — 材質ごとの編集差分（カラー / スカラー差分 ＋ MME カテゴリ上書き）を `popone_history.json` に保存し、リロード時に復元する。
- **MME（ray-mmd）マテリアル生成** — 出力タブの PMX 変換セクションに「MME マテリアル (.fx) も出力」チェックボックスを追加。チェックを入れると PMX 変換時に `<モデル名>_mme/material_<名称>.fx` を書き出す。`CUSTOM_ENABLE` ベースのテンプレート（Standard / Skin / HairAniso / Glass / Cloth / ClearCoat / Emissive）で生成。ray-mmd ルートフォルダはチェックボックス展開時にフォルダ選択ダイアログで設定可能（未設定時はカレントディレクトリ `.\` をフォールバック）。`#include` パスは `pathdiff` + `dunce` 正規化で相対化し、計算不能時はフォールバック。PMX では扱えない補助テクスチャ（normal / emissive / matcap / rim / shading shift）は `<モデル名>_mme/textures/` にコピーして相対パスで参照する。`.fx` と `README.txt` は Shift-JIS + CR+LF でエンコード。`#include` 先（`material_common_2.0.fxsub`）が存在しない場合は変換結果に警告を表示（ファイル出力は継続）。

### 挙動の変更

- **PMX 変換の判定主軸を `ShaderFamily::Mtoon` に切替**。旧来の `is_mtoon()`（`mtoon.is_some()` 判定）は材質編集 UI が非 MToon 材質にも MToon パラメータを表示するため、安直に使うと PMX 出力の ambient / specular が MToon 扱いに変わってしまう。副作用回避のため、UI から明示的に「MToon 有効化」チェックを入れるまで `shader_family` は変更しない。

### テスト

- ユニットテスト 230 件（v0.4.0 の 185 件から増加）。追加カバレッジ: `MaterialParamOverride` の diff/apply ラウンドトリップ、`RayMmdMaterialKind` カテゴリ推定（日本語 / 大小混在 / プレフィックス付き材質名）、`generate_fx` のセクション完全性と CR+LF エンコーディング、`TextureSlot::is_linear` の全バリアントカバレッジ。


## v0.4.0（2026-04-11）

ログビュアー機能を別ウインドウで追加し、「ユーザの明示操作とパニック時以外はログファイルを生成しない」方針へ全面的に切り替えた。

### 新機能

- **ログビュアー（OS 別ウインドウ）** — トップバーの「ログ」ボタンが、インメモリのログバッファをリアルタイムに表示する独立した OS ウインドウを開く動作に変わった。`eframe` の `show_viewport_deferred` を採用しているため、メインの 3D ビューポートとは完全に独立して動き、別モニタへの移動・個別の最小化が可能で、新しいログが流入してもメイン 3D シーンを再描画させない（deferred クロージャ内で約 150ms ポーリング）。
- **レベルフィルタ** — Error / Warn / Info / Debug をチェックボックスで個別に切替。色分け（Error=赤、Warn=黄、Info=白、Debug/Trace=灰、Unknown=薄灰）。バックトレース等のマルチラインメッセージは 1 件として連結して扱う。
- **自動追尾** — トグル ON で末尾スクロールに追従。手動でスクロールアップすると一時停止し、最下部へ戻すと再開。
- **ログの手動保存** — 「ログ保存」ボタンで `rfd::FileDialog` 経由で任意のパスへ `.log` を書き出し。「フォルダを開く」ボタンで `logs/` ディレクトリをエクスプローラで開く。
- **状態の永続化** — ログビュアーの表示状態、ウインドウ位置・サイズ、レベルフィルタ設定を `popone.toml` の `[log_viewer]` セクションに保存し、次回起動時に復元する。

### 挙動の変更

- **通常終了時にログファイルを自動生成しなくなった。** 従来は通常終了時にインメモリログを `popone_<ts>.log` へフラッシュしていたが、v0.4.0 ではこれを廃止。バッファはメモリに残ったままプロセス終了で破棄される。セッションのログを残したい場合は新設の「ログ保存」ボタンを使う。
- **パニックダンプを `panic_<ts>.log` へ直接書き込み。** 従来は「`popone_<ts>.log` に書いてから `panic_<ts>.log` にコピー」という経路でクラッシュごとに 2 ファイル生成していたが、`panic_<ts>.log` を直接書き出す形に変更し、1 クラッシュ＝1 ファイルに揃えた。
- **ログ自動ローテーションを廃止。** `rotate_logs` と `[log] keep` 設定を削除。`%LOCALAPPDATA%\popone\logs\` 配下のファイルは「ユーザの明示操作（手動エクスポート）またはパニック」の結果のみ存在するようになったため、自動削除すべきではないという判断。既存 `popone.toml` の `[log] keep = N` 行は serde が未知フィールドを無視するため、後方互換性は維持される。

### 内部実装

- 新規モジュール `popone/src/viewer/log_viewer.rs` を追加。`[HH:MM:SS.mmm][LEVEL] message` 形式の手書きパーサ、リングバッファ（20,000 行上限）、インクリメンタル更新の `filter_indices`、17 件のユニットテスト（マルチライン連結、バイト単位 drain による先頭断片破棄、CRLF、レベルフィルタ往復、起動時/同セッション reopen での geometry 復元など）を含む。
- `LogViewerModel` は `Arc<Mutex<LogViewerModel>>` で保持し、`show_viewport_deferred` のクロージャ（`Fn + Send + Sync + 'static` 制約あり）に `Arc::clone` で渡せるようにした。
- ウインドウの位置・サイズは毎フレーム子 viewport の入力からキャプチャし、同セッション内の開閉・プロセス再起動の両方で位置が正しく往復するよう実装した。

## v0.3.0（2026-04-11）

初回公開リリース。ドキュメント MECE 再編、UX 改善、UnityPackage 周りのバグ修正を中心に構成。
