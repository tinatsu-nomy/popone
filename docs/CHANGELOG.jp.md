<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [更新履歴](#%E6%9B%B4%E6%96%B0%E5%B1%A5%E6%AD%B4)
  - [v0.5.1（2026-04-13）](#v0512026-04-13)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD)
    - [パフォーマンス](#%E3%83%91%E3%83%95%E3%82%A9%E3%83%BC%E3%83%9E%E3%83%B3%E3%82%B9)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85)
    - [バグ修正（リリース前レビュー対応）](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3%E3%83%AA%E3%83%AA%E3%83%BC%E3%82%B9%E5%89%8D%E3%83%AC%E3%83%93%E3%83%A5%E3%83%BC%E5%AF%BE%E5%BF%9C)
    - [テスト](#%E3%83%86%E3%82%B9%E3%83%88)
    - [v0.6.0 に延期](#v060-%E3%81%AB%E5%BB%B6%E6%9C%9F)
  - [v0.5.0（2026-04-13）](#v0502026-04-13)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-1)
    - [挙動の変更](#%E6%8C%99%E5%8B%95%E3%81%AE%E5%A4%89%E6%9B%B4)
    - [テスト](#%E3%83%86%E3%82%B9%E3%83%88-1)
  - [v0.4.0（2026-04-11）](#v0402026-04-11)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-2)
    - [挙動の変更](#%E6%8C%99%E5%8B%95%E3%81%AE%E5%A4%89%E6%9B%B4-1)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85-1)
  - [v0.3.0（2026-04-11）](#v0302026-04-11)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# 更新履歴

[English](CHANGELOG.md)

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
