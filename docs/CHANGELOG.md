# 更新履歴

[English](CHANGELOG.en.md)

## v0.2.5

### 改善

- **テクスチャ自動縮小** — GPU の最大テクスチャサイズ（通常 8192px）を超えるテクスチャを自動的にアスペクト比を保って縮小。巨大テクスチャを含むモデルでのクラッシュを防止
- **アーカイブ直接ロード（ZIP / 7z）** — ZIP / 7z アーカイブを直接 D&D / ダイアログで開き、内部の VRM / FBX / PMX / PMD モデルを自動検出。複数モデル時は選択ダイアログを表示。PMX/PMD はテクスチャ参照パスを解析して関連ファイルを自動収集
- **CLI アーカイブ対応** — `popone archive.zip output.pmx` で直接変換。`--list-models` でモデル一覧表示、`--model-name` で指定モデルを選択（完全一致→前方一致→部分一致、各段階で一意のみ採用）
- **Shift_JIS ファイル名対応** — ZIP 内の日本語ファイル名を UTF-8 → Shift_JIS フォールバックで正しくデコード
- **zip bomb 対策** — 総展開サイズ 2GB 上限、ZIP は `take()` でハード制限、7z はチャンク読み込みで実読込バイト数を検証
- **パストラバーサル防御** — アーカイブ内の `..` を含むパスを拒否（ZipSlip 攻撃対策）
- **リロード対応** — アーカイブから読み込んだモデルの Aスタンス切替等のリロードに対応。`ReloadableSource::Archive` で選択モデルパスを保持
- **アーカイブ内 UnityPackage 対応** — ZIP / 7z 内の `.unitypackage` を自動検出し、二重展開で内部の VRM / FBX を読み込み。リロード・アペンド・テクスチャ復元にも対応
- **展開サイズ上限** — `.unitypackage` (tar.gz) の展開にも 2GB サイズ上限を適用。外側アーカイブと内側パッケージの両方で防御
- **スタンス変換警告の常時表示** — Aスタンス/Tスタンス変換をONにしたが変換が実行されなかった場合、ビューポート左下に常時警告を表示。腕ボーン未検出（赤文字）/ 既に目標姿勢に近い（黄文字）の2種を表示。PMX出力時の警告もA/Tスタンスに応じて文言を分岐
- **UVマップ PSD レイヤーグループ化** — 複数モデルをマージした場合、UVマップ PSD 出力でモデル別にレイヤーをグループフォルダに格納。単一モデルでもグループ化される。PSD の lsct (Section Divider Setting) を使用し、Photoshop / CLIP STUDIO Paint と互換
- **MaterialGroup 構造体** — ビューアの材質グループ管理を `(String, usize, usize)` タプルから `MaterialGroup` 構造体に変更。`material_range`（材質index範囲）と `draw_range`（DrawCall範囲）を分離し、UV出力とUI表示で適切な範囲を使用

### コード品質・パフォーマンス改善

- **構造化エラー型** — `thiserror` で `PoponeError` enum を定義し、公開 API を `error::Result` に移行。内部は `anyhow` を継続使用し `From<anyhow::Error>` ブリッジで互換性維持
- **ViewerApp 構造体分割** — `PendingState`（遅延処理 10 フィールド）と `ExportState`（PMX エクスポート 4 フィールド）を分離。フィールド数 43 → 27 に削減
- **毎フレーム GPU テクスチャ再登録回避** — ビューポートテクスチャの登録/解放を `update_egui_texture_from_wgpu_texture` に変更し、フレームレート改善
- **ステータスバー format! キャッシュ** — モデル統計文字列をロード時に事前フォーマットし、毎フレームのヒープ割り当てを回避
- **リロード時の clone → take** — `reload_current()` で `morph_weights`・`material_visibility` 等を `std::mem::take()` で所有権移動（ヒープ再割り当て回避）
- **GLB 二重読み込み回避** — VRM 変換時に GLB を `(ir, glb_for_tex)` タプルで保持し、テクスチャ書き出し用の再読み込みを排除
- **BindGroupLayout 共通関数化** — `gpu::create_material_bind_group_layout()` で材質用レイアウト定義を一元化
- **dump コード重複解消** — `dump_ir()` 関数を抽出し、`run_main` と `run_archive_convert` の重複を除去

<details>
<summary>内部改善の詳細</summary>

#### 構造化エラー型（thiserror）

`error.rs` に `PoponeError` enum を定義し、`lib.rs` の公開 API を `error::Result` に移行。

```rust
#[derive(Debug, thiserror::Error)]
pub enum PoponeError {
    #[error("ファイル読み込み失敗: {0}")]
    Io(#[from] std::io::Error),
    #[error("GLB/VRM パース失敗: {0}")]
    GltfParse(#[from] gltf::Error),
    #[error("FBX パース失敗: {0}")]
    FbxParse(String),
    // ... PmxParse, PmdParse, Extraction, Build, Texture, Image, UnityPackage, Archive, Other
}

/// anyhow::Error から PoponeError への変換（既存コードとの互換用）
impl From<anyhow::Error> for PoponeError { ... }

pub type Result<T> = std::result::Result<T, PoponeError>;
```

- 公開 API: `error::Result<T>`（`PoponeError` で構造化）
- 内部: `anyhow::Result` を継続使用（`bail!`、`context()` 等の利便性を維持）
- ブリッジ: `From<anyhow::Error> for PoponeError` で `?` 演算子が自動変換

#### ViewerApp さらなる構造体分離

v0.2.2 の `TextureState` / `AnimLibrary` に加え、`PendingState` / `ExportState` を分離:

| サブ構造体 | フィールド | アクセス | 内容 |
|-----------|----------|---------|------|
| `TextureState` | `self.tex.*` | 9 フィールド | テクスチャ割り当て・パッケージテクスチャ・プレビュー・マッチング |
| `AnimLibrary` | `self.anim.*` | 4 フィールド | アニメーション再生状態・ライブラリ・Muscle スケール |
| `PendingState` | `self.pending.*` | 10 フィールド | 遅延処理（ファイル読み込み・GPU 再構築・PMX 変換等） |
| `ExportState` | `self.export.*` | 4 フィールド | PMX エクスポート（出力パス・ログ・表示材質のみ・UV 解像度） |

ViewerApp のフィールド数: 43（v0.2.1）→ 30（v0.2.2）→ 27（v0.2.5）。

#### 毎フレーム GPU テクスチャ再登録回避

ビューポートのオフスクリーンテクスチャ登録を初回 `register` + 以降 `update` 方式に変更:

```rust
// 変更前: 毎フレーム free + register
egui_renderer.free_texture(&old_id);
let tex_id = egui_renderer.register_native_texture(device, &view, FilterMode::Linear);

// 変更後: 初回 register、以降 update
let tex_id = if let Some(existing_id) = *cached_id {
    egui_renderer.update_egui_texture_from_wgpu_texture(device, &view, FilterMode::Linear, existing_id);
    existing_id
} else {
    let id = egui_renderer.register_native_texture(device, &view, FilterMode::Linear);
    *cached_id = Some(id);
    id
};
```

#### ステータスバー format! キャッシュ

モデル統計文字列を `CachedStats::new()` でロード時に事前フォーマット:

```rust
pub struct CachedStats {
    pub total_vertices: usize,
    pub total_faces: usize,
    pub status_text: String,  // 事前フォーマット済み
}

impl CachedStats {
    fn new(ir: &IrModel) -> Self {
        let status_text = format!(
            "頂点:{} 面:{} 材質:{} テクスチャ:{} ボーン:{} モーフ:{}",
            ...
        );
        Self { total_vertices, total_faces, status_text }
    }
}
```

`CachedMaterialInfo` にも `tex_status_text` フィールドを追加し、テクスチャ設定状況の文字列もキャッシュ。

#### リロード時の clone → take

`reload_current()` で状態を退避する際、`clone()` を `std::mem::take()` に変更:

| 対象 | 変更前 | 変更後 |
|------|--------|--------|
| `morph_weights` | `.clone()` | `std::mem::take()` |
| `material_visibility` | `.clone()` | `std::mem::take()` |
| `material_filter` | `.clone()` | `std::mem::take()` |
| `pmx_output_path` | `.clone()` | `std::mem::take()` |
| `tex.assignments` | `.clone()` | `std::mem::take()` |
| `tex.pkg_assignments` | `.clone()` | `std::mem::take()` |

`take()` は所有権を移動するため、Vec / HashMap のヒープ再割り当てが発生しない。リロード成功後に同じデータを復元するため、移動元は空の状態で問題ない。

#### GLB 二重読み込み回避

CLI 変換（`run_main`）で VRM → PMX 変換時、GLB を 2 回読み込んでいた問題を修正:

```rust
// 変更前: extract + テクスチャ書き出しで2回読み込み
let ir = vrm::extract::extract_ir_model(...)?;
let glb = vrm::loader::load_glb(&input)?;  // 2回目
convert::texture::write_all_textures(&ir.textures, &glb.images, &tex_dir)?;

// 変更後: タプルで保持して再利用
let (mut ir, glb_for_tex) = match ext.as_str() {
    _ => {
        let glb = vrm::loader::load_glb(&input)?;
        let ir = vrm::extract::extract_ir_model(...)?;
        (ir, Some(glb))
    }
};
if let Some(ref glb) = glb_for_tex {
    convert::texture::write_all_textures(&ir.textures, &glb.images, &tex_dir)?;
}
```

#### aux_files clone → take

`take_or_collect_aux()` で `preloaded.aux_files` を `clone()` から `take()` に変更し、HashMap バケットの再割り当てを回避。`preloaded` には空の HashMap を入れ直し、`main_bytes` は保持。

#### BindGroupLayout 共通関数化

材質用 `BindGroupLayout` の descriptor 定義を `gpu::create_material_bind_group_layout()` に一元化し、`gpu.rs` と `mesh.rs` のコード重複を解消。

#### dump コード重複解消

`run_main` と `run_archive_convert` に重複していたダンプ出力コードを `dump_ir()` 関数に抽出。

</details>

## v0.2.4

### 改善

- **アーカイブD&Dリロード対応** — zip/7z から D&D したファイルがOS一時ディレクトリに展開される問題に対応。モデル本体＋補助ファイル（テクスチャ・.txt）をオンメモリにスナップショット保持し、一時ファイル消失後もリロード可能に。VRM/FBX/PMX/PMD 全対応
- **アーカイブD&D先読みキャッシュ** — D&D 検出時点でモデル本体＋隣接テクスチャのバイト列を `PreloadedData` に先読み。以降のロードチェーン全体でキャッシュを使用し、一時ファイル消失後も確実にロード。FBX 選択ダイアログ（`PendingFbxChoice`）を挟む経路でもデータを受け渡し。VRM/FBX/PMX/PMD/UnityPackage 全形式対応
- **アーカイブD&D即座ロード** — zipアーカイブからの D&D 時、一時ファイルが2フレーム遅延の間に消失するエラーを修正。一時パスを検出した場合はプログレスオーバーレイを省略して即座にロード
- **テクスチャD&Dキャッシュ** — ZIP 内テクスチャの D&D 時、プレビュー段階でバイトデータ・PSD 判定・一時パスフラグをキャッシュ。確定時のファイル再読み込みを排除し、一時ファイル消失後もテクスチャ割り当てが確実に記録される
- **UnityPackage アーカイブスナップショット** — ZIP 内 .unitypackage の D&D 時、アーカイブデータを `Arc<[u8]>` でスナップショット保持。リロード・アペンド時に一時ファイルに依存せずメモリから復元可能に
- **シェーダー対応PMX材質** — MToon の shade_color と diffuse の輝度比に基づくトゥーンテクスチャ自動選択（5段階）。MToon 材質の ambient を shade_color ベースに、specular をゼロに補正。非 MToon は従来動作を維持
- **Aスタンス変換警告** — PMX 変換時、Aスタンス変換が有効だが腕ボーンが見つからない場合に赤文字オーバーレイで警告を表示。既にAスタンスに近い場合はスキップ通知を表示
- **ConvertResult::Warning** — 変換成功だが注意事項がある場合の新しいメッセージ種別（赤文字表示、Failure とは区別）
- **AStanceResult enum** — Aスタンス変換結果を型安全に管理（NotRequested / Applied / AlreadyAStance / NotFound）。IrModel::merge() での統合ロジック付き
- **リロード時テクスチャ正規化** — UnityPackage リロード時の PSD→PNG 変換バイパスを修正。MIME タイプ設定も正規パスと統一
- **IrTexture 重複排除** — テクスチャ割り当て時に filename + data で同一性を判定し、同一テクスチャの重複追加を防止

## v0.2.3

### 改善

- **表示材質のみ出力** — PMX 変換時に、表示タブで非表示にした材質を出力から除外するオプション（デフォルト OFF）。材質・メッシュ・テクスチャ・頂点モーフ・グループモーフを一貫してフィルタリング
- **ボーンマージ 2パス方式** — 同名ボーン統合の親子判定を順序非依存の候補収集＋伝播ループに変更。異なる部分木の子孫が誤統合されるバグを修正
- **pkg テクスチャ名前空間** — 複数 UnityPackage 追加時のテクスチャ名衝突を防止（`{パッケージ名}_pkg{連番}_{テクスチャ名}` 形式）。auto-matched テクスチャにも適用
- **ASCII FBX Content 処理** — Content ブロックを文字列として保持し、パーサー層の完全性を維持
- **テスト 61 件** — ボーンマージ・物理リマップ・モーフオフセット・エクスポートフィルタ等のテストを追加

## v0.2.2

### コード品質・パフォーマンス改善

- **パフォーマンス最適化** — アニメーション頂点バッファの毎フレーム alloc 除去、ボーン名探索の HashMap O(1) 化、GPU 可視化バッファの dirty flag 導入
- **テスト拡充** — 10 テスト → 51 テスト。座標変換ラウンドトリップ、ボーン名マッピング、PMX 書き込み・読み込みラウンドトリップ、VRM→PMX E2E テスト
- **Clippy 警告ゼロ** — `cargo clippy --all-targets --all-features -- -D warnings` 完全クリーン
- **UX 改善** — D&D オーバーレイ 4 パターン化、操作ヒント 2 行分割、グレーアウト UI ツールチップ追加

<details>
<summary>内部改善の詳細</summary>

#### ViewerApp サブ構造体化

v0.2.2 で ViewerApp の 43 フィールドを 30 フィールドに削減:

| サブ構造体 | フィールド | アクセス | 内容 |
|-----------|----------|---------|------|
| `TextureState` | `self.tex.*` | 9 フィールド | テクスチャ割り当て・パッケージテクスチャ・プレビュー・マッチング |
| `AnimLibrary` | `self.anim.*` | 4 フィールド | アニメーション再生状態・ライブラリ・Muscle スケール |

Rust の部分借用により `&mut self.tex` と `&self.anim` を同時に借用可能。

#### GPU 可視化バッファのキャッシュ戦略

ボーン・物理・ジョイントの可視化頂点を dirty flag で管理:

| 入力 | キャッシュキー | 再生成条件 |
|------|-------------|----------|
| ボーン頂点 | `camera.eye()`, `bone_opacity` | カメラ移動 / 不透明度変更 / アニメーション再生中 |
| SpringBone 頂点 | `spring_bone_opacity`, `align_rigid_rotation` | 設定変更 / アニメーション再生中 |
| ジョイント頂点 | `joint_opacity` | 設定変更 / アニメーション再生中 |

全バッファ共通:
- `vertex_count == 0` → 強制再生成（非表示→表示トグル復帰）
- `cache_had_anim && !has_anim` → アニメーション解除時に1フレーム強制再生成

#### アニメーション頂点バッファ最適化

`apply_bone_animation()` のホットパス改善:

| 項目 | Before | After |
|------|--------|-------|
| 頂点バッファ | `base.to_vec()` 毎フレーム alloc | `reset_animated_to_base()` capacity 再利用 |
| デルタ行列 | `Vec::with_capacity()` 毎フレーム | `work_deltas` フィールドで再利用 |
| globals 計算 | `Vec` 新規生成 + clone | in-place 更新（`work_computed` フラグ再利用） |
| モーフ適用 | `apply_morphs_to_buf(&self, &mut [Vertex])` | `apply_morphs_to_animated(&mut self)` 借用衝突回避 |

#### ボーン名探索 HashMap 化

`insert_standard_bones()` 内の O(n) 線形探索を HashMap O(1) に:

```rust
// ボーン名 → インデックスの逆引き（重複名は最初の出現を保持）
fn build_bone_map(bones: &[PmxBone]) -> HashMap<String, usize> {
    let mut map = HashMap::with_capacity(bones.len());
    for (i, b) in bones.iter().enumerate() {
        map.entry(b.name.clone()).or_insert(i);
    }
    map
}
```

ボーン配列の変更（挿入・移動）後に `bone_map = build_bone_map(&model.bones)` で再構築。

#### テストデータパス解決

統合テストのファイルパスは環境変数で設定可能:

| 優先度 | 解決元 | 例 |
|--------|-------|-----|
| 1 | ファイル個別環境変数 | `POPONE_TEST_VRM_SEED_SAN=/path/to/Seed-san.vrm` |
| 2 | ルート環境変数 + 相対パス | `POPONE_TEST_DATA=/fixtures` → `/fixtures/vrm-spec/.../Seed-san.vrm` |
| 3 | `CARGO_MANIFEST_DIR/..` | ローカル開発時のデフォルト |

</details>

## FBX 対応

- バイナリ / ASCII FBX の自前パーサー（シーングラフ・座標系自動変換・PreRotation・UnitScaleFactor）
- ASCII FBX: Content ブロック（埋め込みテクスチャ）は文字列として保持し、外部ファイルフォールバックで復元
- スキンウェイト（最大 4 ボーン正規化）、ブレンドシェイプ、UV マッピング
- ヒューマノイドリグ自動検出（Mixamo / 3ds Max Biped / Maya HumanIK / VRoid / Blender）
- 零法線の自動補完、埋め込み/外部テクスチャ対応
