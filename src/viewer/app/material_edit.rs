//! 材質編集ドロワーによる「IR への上書き値」の集約型（Step 2）。
//!
//! Step 1 では diffuse のみを `HashMap<usize, Vec4>` で保持していたが、Step 2 で §E の
//! 各セクション（影 / アウトライン / リム / MatCap / UV アニメ / エミッシブ / 法線 / その他）を
//! 追加するにあたり、全フィールドを 1 つの `MaterialParamOverride` struct に集約する。
//!
//! ## なぜ struct 化するのか
//!
//! Step 2 の各セクション追加時に、毎回 `material_xxx_overrides: HashMap<usize, Type>` を
//! 追加していくとフィールドが 20+ 個に膨れ、初期化・マージ・再適用の一貫性が取れなくなる。
//! `MaterialParamOverride` に集約することで、編集経路・reload 再適用経路を 1 つの `apply_to()`
//! で処理できる。
//!
//! ## Step 3 との関係
//!
//! Step 3 の §I で予定している `MaterialEditRecord.param_override` はまさにこの struct の
//! 将来形で、`declarative_macro` による diff/apply 自動生成に置き換えられる。本ファイルは
//! その「手書き最小版」として先行導入し、Step 3 で macro 化して持続可能な構造に移行する。

use glam::{Vec2, Vec3, Vec4};

use crate::intermediate::types::{
    AlphaMode, CullMode, IrMaterial, IrTextureInfo, MtoonParams, OutlineWidthMode, ShaderFamily,
};

/// スロット単位の KHR_texture_transform 上書き値 (v0.5.4)。
///
/// `IrTextureInfo` が存在するスロット（BaseColor / Emissive / Normal / Shade /
/// ShadingShift / RimMultiply / OutlineWidth / Matcap / UvAnimMask）単位で、
/// `offset / scale / rotation` を差分保存する。全フィールド `None` は「pristine
/// のまま」を意味する。`rotation` は **ラジアン**（UI では度で入力しラジアンで保存）。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TextureUvOverride {
    pub offset: Option<[f32; 2]>,
    pub scale: Option<[f32; 2]>,
    pub rotation: Option<f32>,
}

impl TextureUvOverride {
    pub fn is_empty(&self) -> bool {
        self.offset.is_none() && self.scale.is_none() && self.rotation.is_none()
    }

    /// `pristine` と `current` の IrTextureInfo を比較し、差分のみ `Some(_)` で返す。
    ///
    /// - `(Some, Some)`: 両者を直接比較
    /// - `(None, Some)`: **新規割当スロット**の UV 編集を履歴に載せるため、
    ///   `current` を default transform（offset=0 / scale=1 / rotation=0）と比較する。
    ///   これがないと「未割当スロットにテクスチャを新規割当 → UV を編集」のケースで
    ///   履歴保存時に UV 差分が落ちる（review_result_01 [P1]）。
    /// - `(Some, None)`: スロット自体が解除された状態。texture_mgmt 側でスロット情報が
    ///   消えるため UV diff に意味はなく、`None` を返す。
    /// - `(None, None)`: 変化なし。
    pub fn diff(pristine: Option<&IrTextureInfo>, current: Option<&IrTextureInfo>) -> Option<Self> {
        let c = current?;
        let mut out = Self::default();
        let (p_offset, p_scale, p_rotation) = match pristine {
            Some(p) => (p.offset, p.scale, p.rotation),
            None => (glam::Vec2::ZERO, glam::Vec2::ONE, 0.0),
        };
        if p_offset != c.offset {
            out.offset = Some(c.offset.to_array());
        }
        if p_scale != c.scale {
            out.scale = Some(c.scale.to_array());
        }
        if (p_rotation - c.rotation).abs() > f32::EPSILON {
            out.rotation = Some(c.rotation);
        }
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    /// 既存 `IrTextureInfo` に差分を適用する。対象スロット未割当（`None`）の場合は何もしない。
    pub fn apply(&self, info: &mut Option<IrTextureInfo>) {
        let Some(ti) = info.as_mut() else {
            return;
        };
        if let Some(o) = self.offset {
            ti.offset = Vec2::from_array(o);
        }
        if let Some(s) = self.scale {
            ti.scale = Vec2::from_array(s);
        }
        if let Some(r) = self.rotation {
            ti.rotation = r;
        }
    }
}

/// 材質単位のパラメータ上書き値（`Some(_)` のフィールドだけ IR に書き込まれる）。
///
/// A スタンス変換・T スタンス変換等の reload で IR が再構築されても、`apply_to()` により
/// 新 IR に再適用される。現時点では **diffuse RGB を含む** 全セクションのカラー・スカラー
/// 値のみを保持し、テクスチャ割当（`TextureSlot`）は別経路（`tex.assignments`）で管理する。
///
/// `Default::default()` は全フィールド `None` の「空の上書き」。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MaterialParamOverride {
    // ===== MToon 有効化 (§G / Step 2-10) =====
    //
    // `Some(true)` = ユーザーが「MToon 有効化」チェックを入れた
    //   → `apply_to` で `shader_family = Mtoon` + `mtoon = Some(default)` を設定
    // `Some(false)` = ユーザーが「MToon 有効化」チェックを外した
    //   → `apply_to` で `shader_family = Other` + `mtoon = None` に戻す
    // `None` = 未操作（IR 側の既存値を維持）
    //
    // **適用順序が重要**: `apply_to` では `enable_mtoon` を**最初に**処理してから
    // 他の MToon 系フィールド（shade_color 等）を適用する。そうしないと、先に `mtoon_mut()`
    // が呼ばれて `shader_family` と `mtoon` の整合が取れなくなる。
    pub enable_mtoon: Option<bool>,

    // ===== 基本セクション (§E-1) =====
    pub diffuse: Option<Vec4>,
    pub alpha_mode: Option<AlphaMode>,
    pub alpha_cutoff: Option<f32>,
    pub cull_mode: Option<CullMode>,

    // ===== 影 (Shade) セクション (§E-2) =====
    /// `MtoonParams.shade_color` は `Option<Vec3>` なので、ここでは `Option<Option<Vec3>>` ではなく
    /// 「Some なら `MtoonParams.shade_color = Some(_)` に設定する」意味の `Option<Vec3>` を使う。
    /// `None` を設定する経路（shade を明示的に解除）は Step 3 で追加する。
    pub shade_color: Option<Vec3>,
    pub shading_toony_factor: Option<f32>,
    pub shading_shift_factor: Option<f32>,
    pub gi_equalization_factor: Option<f32>,

    // ===== アウトラインセクション (§E-3) =====
    pub edge_color: Option<Vec4>,
    pub edge_size: Option<f32>,
    pub outline_width_mode: Option<OutlineWidthMode>,
    pub outline_width_factor: Option<f32>,
    pub outline_lighting_mix: Option<f32>,

    // ===== リムセクション (§E-4) =====
    pub parametric_rim_color: Option<Vec3>,
    pub parametric_rim_fresnel_power: Option<f32>,
    pub parametric_rim_lift: Option<f32>,
    pub rim_lighting_mix: Option<f32>,

    // ===== MatCap セクション (§E-5) =====
    pub matcap_factor: Option<Vec3>,

    // ===== UV アニメセクション (§E-6) =====
    pub uv_animation_scroll_x_speed: Option<f32>,
    pub uv_animation_scroll_y_speed: Option<f32>,
    pub uv_animation_rotation_speed: Option<f32>,

    // ===== エミッシブ/法線セクション (§E-7) =====
    pub emissive_factor: Option<Vec3>,
    pub normal_texture_scale: Option<f32>,

    // ===== その他セクション (§E-8) =====
    pub render_queue_offset: Option<i32>,

    // ===== MME カテゴリ上書き (§K.3 / Step 6) =====
    /// ユーザーが手動で上書きした ray-mmd カテゴリ。`None` = 推定値を使用。
    pub mme_kind: Option<crate::convert::mme::ray_mmd::RayMmdMaterialKind>,

    // ===== 材質名 (v0.5.3) =====
    /// ユーザーが編集した材質名。`None` = pristine の name をそのまま使用。
    /// `String` は Copy でないため、merge/apply では個別に clone する。
    pub name: Option<String>,

    // ===== スロット毎 UV 変形 (v0.5.4) =====
    //
    // 各スロットが `IrTextureInfo` を持つ場合のみ有効。未割当スロットへの
    // apply は no-op（勝手に `from_index(0)` を挿入しない）。
    // Expression 駆動の UV アニメ (`IrTextureTransformBind`) とは経路が独立で、
    // 静的 override → Expression 加算の順で両立する。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_color_uv: Option<TextureUvOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emissive_uv: Option<TextureUvOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normal_uv: Option<TextureUvOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shade_uv: Option<TextureUvOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shading_shift_uv: Option<TextureUvOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rim_multiply_uv: Option<TextureUvOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outline_width_uv: Option<TextureUvOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matcap_uv: Option<TextureUvOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uv_animation_mask_uv: Option<TextureUvOverride>,
}

impl MaterialParamOverride {
    /// 空の上書き（`Default::default()` のエイリアス）。
    pub fn new() -> Self {
        Self::default()
    }

    /// `other` の `Some(_)` フィールドを自身に取り込む（自身側を上書き）。
    ///
    /// ui.rs の `show_material_editor_window` では 1 フレーム分の編集差分を
    /// `pending_override` に蓄積し、closure 外で `material_overrides[mat_idx]` に
    /// マージする。このときに 24+ フィールド分の `if let Some(v) = ...` を毎回書く
    /// 手間を避けるため、ローカル `merge` macro で全フィールドを一括処理する。
    ///
    /// Step 3 の `declarative_macro` 版 (`define_param_override!`) に置き換えられる
    /// 予定だが、それまでは本メソッドで簡潔に merge できる。
    pub fn merge_from(&mut self, other: &Self) {
        macro_rules! merge {
            ($($f:ident),* $(,)?) => {
                $(
                    if other.$f.is_some() {
                        self.$f = other.$f;
                    }
                )*
            };
        }
        merge!(
            // MToon 有効化
            enable_mtoon,
            // 基本
            diffuse,
            alpha_mode,
            alpha_cutoff,
            cull_mode,
            // 影 (Shade)
            shade_color,
            shading_toony_factor,
            shading_shift_factor,
            gi_equalization_factor,
            // アウトライン
            edge_color,
            edge_size,
            outline_width_mode,
            outline_width_factor,
            outline_lighting_mix,
            // リム
            parametric_rim_color,
            parametric_rim_fresnel_power,
            parametric_rim_lift,
            rim_lighting_mix,
            // MatCap
            matcap_factor,
            // UV アニメ
            uv_animation_scroll_x_speed,
            uv_animation_scroll_y_speed,
            uv_animation_rotation_speed,
            // エミッシブ / 法線
            emissive_factor,
            normal_texture_scale,
            // その他
            render_queue_offset,
            // MME
            mme_kind,
        );
        // String は Copy でないため個別に clone
        if let Some(ref v) = other.name {
            self.name = Some(v.clone());
        }
        // UV override は Option<TextureUvOverride> で Copy でないため個別に clone
        macro_rules! merge_uv {
            ($($f:ident),* $(,)?) => {
                $(
                    if let Some(ref v) = other.$f {
                        self.$f = Some(v.clone());
                    }
                )*
            };
        }
        merge_uv!(
            base_color_uv,
            emissive_uv,
            normal_uv,
            shade_uv,
            shading_shift_uv,
            rim_multiply_uv,
            outline_width_uv,
            matcap_uv,
            uv_animation_mask_uv,
        );
    }

    /// `Some(_)` のフィールドが 1 つでもあれば `true` を返す。
    /// 空の上書きは保存するだけ無駄なので、HashMap 挿入前のガードに使う。
    pub fn is_empty(&self) -> bool {
        self.enable_mtoon.is_none()
            && self.diffuse.is_none()
            && self.alpha_mode.is_none()
            && self.alpha_cutoff.is_none()
            && self.cull_mode.is_none()
            && self.shade_color.is_none()
            && self.shading_toony_factor.is_none()
            && self.shading_shift_factor.is_none()
            && self.gi_equalization_factor.is_none()
            && self.edge_color.is_none()
            && self.edge_size.is_none()
            && self.outline_width_mode.is_none()
            && self.outline_width_factor.is_none()
            && self.outline_lighting_mix.is_none()
            && self.parametric_rim_color.is_none()
            && self.parametric_rim_fresnel_power.is_none()
            && self.parametric_rim_lift.is_none()
            && self.rim_lighting_mix.is_none()
            && self.matcap_factor.is_none()
            && self.uv_animation_scroll_x_speed.is_none()
            && self.uv_animation_scroll_y_speed.is_none()
            && self.uv_animation_rotation_speed.is_none()
            && self.emissive_factor.is_none()
            && self.normal_texture_scale.is_none()
            && self.render_queue_offset.is_none()
            && self.mme_kind.is_none()
            && self.name.is_none()
            && self.base_color_uv.is_none()
            && self.emissive_uv.is_none()
            && self.normal_uv.is_none()
            && self.shade_uv.is_none()
            && self.shading_shift_uv.is_none()
            && self.rim_multiply_uv.is_none()
            && self.outline_width_uv.is_none()
            && self.matcap_uv.is_none()
            && self.uv_animation_mask_uv.is_none()
    }

    /// `pristine`（ロード直後の IR 材質値）と `current`（ユーザー編集後の IR 材質値）を
    /// 比較し、値が異なるフィールドだけ `Some(_)` にした `MaterialParamOverride` を返す。
    /// 全フィールドが一致していれば `None` を返す。
    ///
    /// **永続化での用途**: `popone_history.json` v2 に保存する `MaterialEditRecord.param_override`
    /// は、この diff_from で計算した差分のみを含む。ロード直後の値と同じフィールドは
    /// `None` のまま skip_serializing されるので、ファイルサイズが必要最小限になる。
    ///
    /// **enable_mtoon の diff**: `shader_family` が pristine と current で異なる場合、
    /// `enable_mtoon = Some(current == Mtoon)` を設定する。
    pub fn diff_from(pristine: &IrMaterial, current: &IrMaterial) -> Option<Self> {
        let mut out = Self::default();

        // enable_mtoon: shader_family の差分
        if pristine.shader_family != current.shader_family {
            out.enable_mtoon = Some(matches!(current.shader_family, ShaderFamily::Mtoon));
        }

        // 材質名の差分（String は Copy でないため diff_field macro 対象外）
        if pristine.name != current.name {
            out.name = Some(current.name.clone());
        }

        // 基本
        macro_rules! diff_field {
            ($field:ident, $get:expr) => {
                if $get(pristine) != $get(current) {
                    out.$field = Some($get(current));
                }
            };
        }

        diff_field!(diffuse, |m: &IrMaterial| m.diffuse);
        diff_field!(alpha_mode, |m: &IrMaterial| m.alpha_mode);
        diff_field!(alpha_cutoff, |m: &IrMaterial| m.alpha_cutoff);
        diff_field!(cull_mode, |m: &IrMaterial| m.cull_mode);
        diff_field!(edge_color, |m: &IrMaterial| m.edge_color);
        diff_field!(edge_size, |m: &IrMaterial| m.edge_size);
        diff_field!(emissive_factor, |m: &IrMaterial| m.emissive_factor);
        diff_field!(normal_texture_scale, |m: &IrMaterial| m
            .normal_texture_scale);

        // MToon 系（mtoon() でデフォルト値にフォールバック、副作用なし）
        //
        // review_009 [P2] 対応: enable_mtoon = Some(false)（MToon を OFF にした状態）のとき、
        // MToon 系フィールドの diff を**全スキップ**する。理由:
        // - current.mtoon() は mtoon = None でも MTOON_DEFAULT を返すため、「OFF にした」
        //   にもかかわらず pristine との差分が MToon 系 override として保存されてしまう
        // - apply_to 復元時に enable_mtoon = false で mtoon = None にした直後、MToon 系
        //   override が has_mtoon_override = true を通って mtoon_mut() → Some(default) を
        //   再挿入し、round-trip が壊れる
        //
        // shade_color は Option<Vec3> なので diff_field! macro を使えない
        // （Some(Option<Vec3>) = Option<Option<Vec3>> になってしまう）。直接代入で処理する。
        let diff_mtoon = out.enable_mtoon != Some(false);
        if diff_mtoon {
            let p = pristine.mtoon().shade_color;
            let c = current.mtoon().shade_color;
            if p != c {
                out.shade_color = c;
            }
            diff_field!(shading_toony_factor, |m: &IrMaterial| m
                .mtoon()
                .shading_toony_factor);
            diff_field!(shading_shift_factor, |m: &IrMaterial| m
                .mtoon()
                .shading_shift_factor);
            diff_field!(gi_equalization_factor, |m: &IrMaterial| m
                .mtoon()
                .gi_equalization_factor);
            diff_field!(outline_width_mode, |m: &IrMaterial| m
                .mtoon()
                .outline_width_mode);
            diff_field!(outline_width_factor, |m: &IrMaterial| m
                .mtoon()
                .outline_width_factor);
            diff_field!(outline_lighting_mix, |m: &IrMaterial| m
                .mtoon()
                .outline_lighting_mix);
            diff_field!(parametric_rim_color, |m: &IrMaterial| m
                .mtoon()
                .parametric_rim_color);
            diff_field!(parametric_rim_fresnel_power, |m: &IrMaterial| m
                .mtoon()
                .parametric_rim_fresnel_power);
            diff_field!(parametric_rim_lift, |m: &IrMaterial| m
                .mtoon()
                .parametric_rim_lift);
            diff_field!(rim_lighting_mix, |m: &IrMaterial| m
                .mtoon()
                .rim_lighting_mix);
            diff_field!(matcap_factor, |m: &IrMaterial| m.mtoon().matcap_factor);
            diff_field!(uv_animation_scroll_x_speed, |m: &IrMaterial| m
                .mtoon()
                .uv_animation_scroll_x_speed);
            diff_field!(uv_animation_scroll_y_speed, |m: &IrMaterial| m
                .mtoon()
                .uv_animation_scroll_y_speed);
            diff_field!(uv_animation_rotation_speed, |m: &IrMaterial| m
                .mtoon()
                .uv_animation_rotation_speed);
            diff_field!(render_queue_offset, |m: &IrMaterial| m
                .mtoon()
                .render_queue_offset);
        } // end if diff_mtoon

        // ===== スロット毎 UV (v0.5.4) =====
        //
        // 非 MToon でも参照可能な BaseColor / Emissive / Normal と、MToon 限定の
        // 6 スロットを別扱いする。MToon 限定スロットは enable_mtoon == Some(false)
        // のとき全スキップ（round-trip で mtoon = None 側に不要な差分が残らないよう）。
        out.base_color_uv = TextureUvOverride::diff(
            pristine.base_color_tex_info.as_ref(),
            current.base_color_tex_info.as_ref(),
        );
        out.emissive_uv = TextureUvOverride::diff(
            pristine.emissive_texture.as_ref(),
            current.emissive_texture.as_ref(),
        );
        out.normal_uv = TextureUvOverride::diff(
            pristine.normal_texture.as_ref(),
            current.normal_texture.as_ref(),
        );
        if diff_mtoon {
            let p = pristine.mtoon();
            let c = current.mtoon();
            out.shade_uv =
                TextureUvOverride::diff(p.shade_texture.as_ref(), c.shade_texture.as_ref());
            out.shading_shift_uv = TextureUvOverride::diff(
                p.shading_shift_texture.as_ref(),
                c.shading_shift_texture.as_ref(),
            );
            out.rim_multiply_uv = TextureUvOverride::diff(
                p.rim_multiply_texture.as_ref(),
                c.rim_multiply_texture.as_ref(),
            );
            out.outline_width_uv = TextureUvOverride::diff(
                p.outline_width_texture.as_ref(),
                c.outline_width_texture.as_ref(),
            );
            out.matcap_uv =
                TextureUvOverride::diff(p.matcap_texture.as_ref(), c.matcap_texture.as_ref());
            out.uv_animation_mask_uv = TextureUvOverride::diff(
                p.uv_animation_mask_texture.as_ref(),
                c.uv_animation_mask_texture.as_ref(),
            );
        }

        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    /// 自身の上書きを `mat` に適用する。`Some(_)` のフィールドだけを書き込む。
    ///
    /// **適用順序**:
    /// 1. `enable_mtoon` を最初に処理（`shader_family` と `mtoon` の有無を確定）
    /// 2. 基本フィールド（diffuse 等）と非 MToon フィールド
    /// 3. MToon 系フィールドを `mtoon_mut()` 経由で書き込み（1 で `mtoon = Some(_)` が
    ///    確定しているので、意図通りの状態で適用される）
    ///
    /// **§G との関係**: 通常の MToon 系フィールド適用では `shader_family` を変更しない
    /// が、`enable_mtoon` の明示操作だけは `shader_family` を切替える設計。
    pub fn apply_to(&self, mat: &mut IrMaterial) {
        // 材質名（v0.5.3）
        if let Some(ref v) = self.name {
            mat.name = v.clone();
        }

        // ===== MToon 有効化 (最優先) =====
        // 他の MToon 系フィールド適用より**先**に処理することで、後続の
        // `mtoon_mut()` 呼び出しで不整合が起きないようにする。
        if let Some(enable) = self.enable_mtoon {
            if enable {
                mat.shader_family = ShaderFamily::Mtoon;
                if mat.mtoon.is_none() {
                    mat.mtoon = Some(MtoonParams::default());
                }
            } else {
                mat.shader_family = ShaderFamily::Other;
                mat.mtoon = None;
            }
        }

        // 基本
        if let Some(v) = self.diffuse {
            mat.diffuse = v;
        }
        if let Some(m) = self.alpha_mode {
            mat.alpha_mode = m;
        }
        if let Some(c) = self.alpha_cutoff {
            mat.alpha_cutoff = c;
        }
        if let Some(c) = self.cull_mode {
            mat.cull_mode = c;
        }

        // 非 MToon でもアクセス可能なフィールド
        if let Some(v) = self.edge_color {
            mat.edge_color = v;
        }
        if let Some(v) = self.edge_size {
            mat.edge_size = v;
        }
        if let Some(v) = self.emissive_factor {
            mat.emissive_factor = v;
        }
        if let Some(v) = self.normal_texture_scale {
            mat.normal_texture_scale = v;
        }

        // 非 MToon スロットの UV override (v0.5.4): テクスチャ未割当時は no-op
        if let Some(ref uv) = self.base_color_uv {
            uv.apply(&mut mat.base_color_tex_info);
        }
        if let Some(ref uv) = self.emissive_uv {
            uv.apply(&mut mat.emissive_texture);
        }
        if let Some(ref uv) = self.normal_uv {
            uv.apply(&mut mat.normal_texture);
        }

        // MToon 系フィールド: 1 つでも設定があれば mtoon を初期化してから書き込む
        //
        // review_009 [P2] 対応: enable_mtoon = Some(false) の場合は MToon 系 override を
        // **全スキップ**する。先に mtoon = None にしたのに、ここで mtoon_mut() が走って
        // Some(default) を再挿入してしまう round-trip 不整合を防ぐ。
        if self.enable_mtoon == Some(false) {
            return;
        }
        let has_mtoon_override = self.shade_color.is_some()
            || self.shading_toony_factor.is_some()
            || self.shading_shift_factor.is_some()
            || self.gi_equalization_factor.is_some()
            || self.outline_width_mode.is_some()
            || self.outline_width_factor.is_some()
            || self.outline_lighting_mix.is_some()
            || self.parametric_rim_color.is_some()
            || self.parametric_rim_fresnel_power.is_some()
            || self.parametric_rim_lift.is_some()
            || self.rim_lighting_mix.is_some()
            || self.matcap_factor.is_some()
            || self.uv_animation_scroll_x_speed.is_some()
            || self.uv_animation_scroll_y_speed.is_some()
            || self.uv_animation_rotation_speed.is_some()
            || self.render_queue_offset.is_some();

        if has_mtoon_override {
            let mp = mat.mtoon_mut();
            // 影
            if let Some(v) = self.shade_color {
                mp.shade_color = Some(v);
            }
            if let Some(v) = self.shading_toony_factor {
                mp.shading_toony_factor = v;
            }
            if let Some(v) = self.shading_shift_factor {
                mp.shading_shift_factor = v;
            }
            if let Some(v) = self.gi_equalization_factor {
                mp.gi_equalization_factor = v;
            }
            // アウトライン
            if let Some(v) = self.outline_width_mode {
                mp.outline_width_mode = v;
            }
            if let Some(v) = self.outline_width_factor {
                mp.outline_width_factor = v;
            }
            if let Some(v) = self.outline_lighting_mix {
                mp.outline_lighting_mix = v;
            }
            // リム
            if let Some(v) = self.parametric_rim_color {
                mp.parametric_rim_color = v;
            }
            if let Some(v) = self.parametric_rim_fresnel_power {
                mp.parametric_rim_fresnel_power = v;
            }
            if let Some(v) = self.parametric_rim_lift {
                mp.parametric_rim_lift = v;
            }
            if let Some(v) = self.rim_lighting_mix {
                mp.rim_lighting_mix = v;
            }
            // MatCap
            if let Some(v) = self.matcap_factor {
                mp.matcap_factor = v;
            }
            // UV アニメ
            if let Some(v) = self.uv_animation_scroll_x_speed {
                mp.uv_animation_scroll_x_speed = v;
            }
            if let Some(v) = self.uv_animation_scroll_y_speed {
                mp.uv_animation_scroll_y_speed = v;
            }
            if let Some(v) = self.uv_animation_rotation_speed {
                mp.uv_animation_rotation_speed = v;
            }
            // その他
            if let Some(v) = self.render_queue_offset {
                mp.render_queue_offset = v;
            }
        }

        // MToon スロットの UV override (v0.5.4):
        //
        // `mtoon_mut()` は使わない（MToon が無効な材質に default mtoon を挿入しないため）。
        // `mat.mtoon` が既に Some の場合のみ、既存スロットの IrTextureInfo に書き込む。
        // テクスチャ未割当のスロットに対しては `TextureUvOverride::apply()` が no-op 化する。
        if let Some(ref mut mp) = mat.mtoon {
            if let Some(ref uv) = self.shade_uv {
                uv.apply(&mut mp.shade_texture);
            }
            if let Some(ref uv) = self.shading_shift_uv {
                uv.apply(&mut mp.shading_shift_texture);
            }
            if let Some(ref uv) = self.rim_multiply_uv {
                uv.apply(&mut mp.rim_multiply_texture);
            }
            if let Some(ref uv) = self.outline_width_uv {
                uv.apply(&mut mp.outline_width_texture);
            }
            if let Some(ref uv) = self.matcap_uv {
                uv.apply(&mut mp.matcap_texture);
            }
            if let Some(ref uv) = self.uv_animation_mask_uv {
                uv.apply(&mut mp.uv_animation_mask_texture);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intermediate::types::MtoonParams;

    /// MToon 材質の diff → apply round-trip: 保存前後で材質が等価であること。
    #[test]
    fn test_diff_apply_roundtrip_mtoon() {
        let pristine = IrMaterial::default();
        let mut current = pristine.clone();
        current.diffuse = Vec4::new(1.0, 0.0, 0.0, 1.0);
        current.shader_family = ShaderFamily::Mtoon;
        current.mtoon = Some(MtoonParams {
            shade_color: Some(Vec3::new(0.5, 0.0, 0.0)),
            shading_toony_factor: 0.7,
            ..MtoonParams::default()
        });

        let diff = MaterialParamOverride::diff_from(&pristine, &current);
        assert!(diff.is_some(), "diff should be Some when there are changes");

        // apply: pristine に diff を適用 → current と同じになるべき
        let mut restored = pristine.clone();
        diff.unwrap().apply_to(&mut restored);

        assert_eq!(restored.diffuse, current.diffuse);
        assert_eq!(restored.shader_family, current.shader_family);
        assert!(restored.mtoon.is_some());
        assert_eq!(
            restored.mtoon.as_ref().unwrap().shade_color,
            current.mtoon.as_ref().unwrap().shade_color,
        );
        assert!(
            (restored.mtoon.as_ref().unwrap().shading_toony_factor
                - current.mtoon.as_ref().unwrap().shading_toony_factor)
                .abs()
                < 1e-6,
        );
    }

    /// MToon ON → OFF の round-trip (review_009 [P2]): OFF にした状態を diff → apply で
    /// 復元すると、mtoon = None + shader_family = Other が保持されること。
    #[test]
    fn test_diff_apply_roundtrip_mtoon_off() {
        // pristine: MToon 材質（VRM ロード直後の状態を想定）
        let mut pristine = IrMaterial::default();
        pristine.shader_family = ShaderFamily::Mtoon;
        pristine.mtoon = Some(MtoonParams::default());

        // ユーザーが MToon 有効化チェックを OFF にした状態
        let mut current = pristine.clone();
        current.shader_family = ShaderFamily::Other;
        current.mtoon = None;

        let diff = MaterialParamOverride::diff_from(&pristine, &current);
        assert!(
            diff.is_some(),
            "diff should appear when shader_family changes"
        );

        let diff = diff.unwrap();
        assert_eq!(
            diff.enable_mtoon,
            Some(false),
            "MToon disable should be recorded"
        );

        // apply: pristine に diff を適用 → current と同じ状態になるべき
        let mut restored = pristine.clone();
        diff.apply_to(&mut restored);

        assert_eq!(restored.shader_family, ShaderFamily::Other);
        assert!(
            restored.mtoon.is_none(),
            "mtoon = None が保持されるべき（mtoon_mut() が再挿入してはならない）"
        );
    }

    /// 変更なしの diff → None
    #[test]
    fn test_diff_from_no_change() {
        let mat = IrMaterial::default();
        let diff = MaterialParamOverride::diff_from(&mat, &mat);
        assert!(
            diff.is_none(),
            "diff should be None when there are no changes"
        );
    }

    // ===== Step 7-35: diff_from/apply_to テスト拡充 =====

    /// 個別フィールド差分: diffuse のみ変更
    #[test]
    fn test_diff_from_diffuse_only() {
        let pristine = IrMaterial::default();
        let mut current = pristine.clone();
        current.diffuse = Vec4::new(0.5, 0.3, 0.1, 0.9);

        let diff = MaterialParamOverride::diff_from(&pristine, &current).unwrap();
        assert_eq!(diff.diffuse, Some(Vec4::new(0.5, 0.3, 0.1, 0.9)));
        // 他のフィールドは None
        assert!(diff.emissive_factor.is_none());
        assert!(diff.enable_mtoon.is_none());
    }

    /// emissive_factor のみ変更
    #[test]
    fn test_diff_from_emissive_only() {
        let pristine = IrMaterial::default();
        let mut current = pristine.clone();
        current.emissive_factor = Vec3::new(1.0, 0.5, 0.0);

        let diff = MaterialParamOverride::diff_from(&pristine, &current).unwrap();
        assert_eq!(diff.emissive_factor, Some(Vec3::new(1.0, 0.5, 0.0)));
        assert!(diff.diffuse.is_none());
    }

    /// mme_kind は IrMaterial に含まれないため diff_from では生成されない
    #[test]
    fn test_diff_from_never_sets_mme_kind() {
        let pristine = IrMaterial::default();
        let mut current = pristine.clone();
        current.diffuse = Vec4::new(1.0, 0.0, 0.0, 1.0);

        let diff = MaterialParamOverride::diff_from(&pristine, &current).unwrap();
        assert!(
            diff.mme_kind.is_none(),
            "mme_kind は diff_from では設定されない"
        );
    }

    /// is_empty: 全フィールド None なら true
    #[test]
    fn test_is_empty_default() {
        let ov = MaterialParamOverride::default();
        assert!(ov.is_empty());
    }

    /// is_empty: 1 フィールドでも Some なら false
    #[test]
    fn test_is_empty_with_mme_kind() {
        let mut ov = MaterialParamOverride::default();
        ov.mme_kind = Some(crate::convert::mme::ray_mmd::RayMmdMaterialKind::Skin);
        assert!(!ov.is_empty());
    }

    /// merge_from: Some フィールドのみ上書き
    #[test]
    fn test_merge_from_selective() {
        let mut base = MaterialParamOverride::default();
        base.diffuse = Some(Vec4::new(1.0, 0.0, 0.0, 1.0));

        let patch = MaterialParamOverride {
            emissive_factor: Some(Vec3::new(0.5, 0.5, 0.5)),
            ..Default::default()
        };

        base.merge_from(&patch);
        // diffuse は変わらず
        assert_eq!(base.diffuse, Some(Vec4::new(1.0, 0.0, 0.0, 1.0)));
        // emissive_factor はマージされた
        assert_eq!(base.emissive_factor, Some(Vec3::new(0.5, 0.5, 0.5)));
    }

    /// enable_mtoon=false 時に MToon フィールドが diff に含まれないこと
    #[test]
    fn test_diff_from_mtoon_off_skips_mtoon_fields() {
        let mut pristine = IrMaterial::default();
        pristine.shader_family = ShaderFamily::Mtoon;
        pristine.mtoon = Some(MtoonParams {
            shading_toony_factor: 0.9,
            ..MtoonParams::default()
        });

        let mut current = pristine.clone();
        current.shader_family = ShaderFamily::Other;
        current.mtoon = None;

        let diff = MaterialParamOverride::diff_from(&pristine, &current).unwrap();
        assert_eq!(diff.enable_mtoon, Some(false));
        // MToon フィールドはスキップされる
        assert!(diff.shade_color.is_none());
        assert!(diff.shading_toony_factor.is_none());
    }

    // ===== v0.5.4: スロット毎 UV 変形 =====

    /// TextureUvOverride::default() は is_empty が true で、余計な diff 差分を生まない
    #[test]
    fn test_uv_override_default_is_empty() {
        let ov = TextureUvOverride::default();
        assert!(ov.is_empty());
        assert!(MaterialParamOverride::default().is_empty());
    }

    /// BaseColor の UV offset/scale/rotation round-trip: diff → apply で値が復元される
    #[test]
    fn test_diff_apply_roundtrip_base_color_uv() {
        let mut pristine = IrMaterial::default();
        pristine.base_color_tex_info = Some(IrTextureInfo::from_index(0));

        let mut current = pristine.clone();
        {
            let ti = current.base_color_tex_info.as_mut().unwrap();
            ti.offset = Vec2::new(0.25, -0.5);
            ti.scale = Vec2::new(2.0, 0.5);
            ti.rotation = std::f32::consts::FRAC_PI_4;
        }

        let diff = MaterialParamOverride::diff_from(&pristine, &current)
            .expect("diff should be Some because of UV changes");
        let uv = diff
            .base_color_uv
            .as_ref()
            .expect("base_color_uv should be Some");
        assert_eq!(uv.offset, Some([0.25, -0.5]));
        assert_eq!(uv.scale, Some([2.0, 0.5]));
        assert_eq!(uv.rotation, Some(std::f32::consts::FRAC_PI_4));

        let mut restored = pristine.clone();
        diff.apply_to(&mut restored);
        let ti = restored.base_color_tex_info.as_ref().unwrap();
        assert_eq!(ti.offset, Vec2::new(0.25, -0.5));
        assert_eq!(ti.scale, Vec2::new(2.0, 0.5));
        assert!((ti.rotation - std::f32::consts::FRAC_PI_4).abs() < 1e-6);
    }

    /// MToon スロット (shade) の UV round-trip
    #[test]
    fn test_diff_apply_roundtrip_mtoon_slot_uv() {
        let mut pristine = IrMaterial::default();
        pristine.shader_family = ShaderFamily::Mtoon;
        let mut mp = MtoonParams::default();
        mp.shade_texture = Some(IrTextureInfo::from_index(3));
        pristine.mtoon = Some(mp);

        let mut current = pristine.clone();
        {
            let ti = current
                .mtoon
                .as_mut()
                .unwrap()
                .shade_texture
                .as_mut()
                .unwrap();
            ti.offset = Vec2::new(0.1, 0.2);
            ti.scale = Vec2::new(1.5, 1.5);
        }

        let diff = MaterialParamOverride::diff_from(&pristine, &current).unwrap();
        let uv = diff.shade_uv.as_ref().expect("shade_uv should be Some");
        assert_eq!(uv.offset, Some([0.1, 0.2]));
        assert_eq!(uv.scale, Some([1.5, 1.5]));

        let mut restored = pristine.clone();
        diff.apply_to(&mut restored);
        let ti = restored
            .mtoon
            .as_ref()
            .unwrap()
            .shade_texture
            .as_ref()
            .unwrap();
        assert_eq!(ti.offset, Vec2::new(0.1, 0.2));
        assert_eq!(ti.scale, Vec2::new(1.5, 1.5));
    }

    /// 未割当スロットへの apply は no-op（クラッシュせず、勝手に IrTextureInfo を挿入しない）
    #[test]
    fn test_uv_apply_to_unassigned_slot_is_noop() {
        let pristine = IrMaterial::default();
        assert!(pristine.base_color_tex_info.is_none());
        assert!(pristine.emissive_texture.is_none());

        let mut ov = MaterialParamOverride::default();
        ov.base_color_uv = Some(TextureUvOverride {
            offset: Some([1.0, 1.0]),
            scale: Some([2.0, 2.0]),
            rotation: Some(1.0),
        });
        ov.emissive_uv = Some(TextureUvOverride {
            offset: Some([-0.5, 0.3]),
            ..Default::default()
        });

        let mut restored = pristine.clone();
        ov.apply_to(&mut restored);
        // スロットは None のまま（勝手に from_index(0) を挿入していない）
        assert!(restored.base_color_tex_info.is_none());
        assert!(restored.emissive_texture.is_none());
    }

    /// enable_mtoon = Some(false) のとき MToon スロット UV も diff から除外される
    #[test]
    fn test_diff_from_mtoon_off_skips_mtoon_slot_uv() {
        let mut pristine = IrMaterial::default();
        pristine.shader_family = ShaderFamily::Mtoon;
        let mut mp = MtoonParams::default();
        mp.shade_texture = Some(IrTextureInfo::from_index(0));
        pristine.mtoon = Some(mp);

        let mut current = pristine.clone();
        current.shader_family = ShaderFamily::Other;
        current.mtoon = None;

        let diff = MaterialParamOverride::diff_from(&pristine, &current).unwrap();
        assert_eq!(diff.enable_mtoon, Some(false));
        assert!(
            diff.shade_uv.is_none(),
            "MToon OFF 時は shade_uv が diff に含まれない"
        );
    }

    /// MToon 有効で mtoon_mut() を呼ばずに UV override が適用される
    /// （非 MToon 材質に勝手に mtoon = Some(default) が挿入されないこと）
    #[test]
    fn test_apply_uv_does_not_upgrade_to_mtoon() {
        let pristine = IrMaterial::default();
        assert!(pristine.mtoon.is_none());

        let mut ov = MaterialParamOverride::default();
        ov.shade_uv = Some(TextureUvOverride {
            offset: Some([0.5, 0.5]),
            ..Default::default()
        });

        let mut restored = pristine.clone();
        ov.apply_to(&mut restored);
        // mtoon が None のままであることが重要（UV のために mtoon を生成しない）
        assert!(
            restored.mtoon.is_none(),
            "UV override 単体では mtoon を生成してはならない"
        );
    }

    /// review_result_01 [P1]: 未割当スロットに新規割当 + UV 編集した差分が diff に載る
    ///
    /// pristine: slot=None, current: slot=Some(IrTextureInfo{offset=(0.5,0)})
    /// → UV override として `{offset: Some([0.5, 0.0])}` が保存される。
    /// これがないと「テクスチャ新規割当 → UV 編集」ケースの履歴が欠落する。
    #[test]
    fn test_diff_from_newly_assigned_slot_uv_is_saved() {
        let pristine = IrMaterial::default();
        assert!(pristine.base_color_tex_info.is_none());

        let mut current = pristine.clone();
        let mut ti = IrTextureInfo::from_index(5);
        ti.offset = Vec2::new(0.5, 0.0);
        ti.rotation = 0.25;
        current.base_color_tex_info = Some(ti);

        let diff =
            MaterialParamOverride::diff_from(&pristine, &current).expect("Some because of UV diff");
        let uv = diff
            .base_color_uv
            .as_ref()
            .expect("UV diff for newly assigned slot should be saved");
        assert_eq!(uv.offset, Some([0.5, 0.0]));
        assert_eq!(uv.rotation, Some(0.25));
        // default scale = 1.0 のままなので scale は None
        assert_eq!(uv.scale, None);
    }

    /// 逆方向: pristine=Some, current=None（割当解除）では UV diff は None
    /// （スロット情報そのものが texture_mgmt 側で消えるため）
    #[test]
    fn test_diff_from_removed_slot_no_uv_diff() {
        let mut pristine = IrMaterial::default();
        let mut ti = IrTextureInfo::from_index(0);
        ti.offset = Vec2::new(0.3, 0.3);
        pristine.base_color_tex_info = Some(ti);

        let mut current = pristine.clone();
        current.base_color_tex_info = None;

        // base_color_tex_info 以外の差分がないので、UV 単体では None
        let diff = MaterialParamOverride::diff_from(&pristine, &current);
        // current の slot が None なら UV diff は None
        if let Some(d) = diff {
            assert!(d.base_color_uv.is_none());
        }
    }

    /// merge_from で UV override が上書きマージされる
    #[test]
    fn test_merge_from_uv_override() {
        let mut base = MaterialParamOverride::default();
        base.base_color_uv = Some(TextureUvOverride {
            offset: Some([0.0, 0.0]),
            scale: Some([1.0, 1.0]),
            rotation: None,
        });

        let patch = MaterialParamOverride {
            base_color_uv: Some(TextureUvOverride {
                offset: Some([0.5, 0.0]),
                scale: None,
                rotation: Some(0.5),
            }),
            ..Default::default()
        };
        base.merge_from(&patch);
        // base の値が patch で上書きされる（`Option<TextureUvOverride>` 単位の置換）
        let uv = base.base_color_uv.as_ref().unwrap();
        assert_eq!(uv.offset, Some([0.5, 0.0]));
        assert_eq!(uv.rotation, Some(0.5));
    }
}
