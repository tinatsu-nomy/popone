use std::sync::Arc;

use glam::{Mat4, Vec2, Vec3, Vec4};

/// Texture UV-wrapping mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IrWrapMode {
    Repeat,
    ClampToEdge,
    MirroredRepeat,
}

/// Texture magnification filter mode (mag_filter).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IrMagFilter {
    Nearest,
    Linear,
}

/// Texture minification filter mode (min_filter + mipmap_filter).
/// Preserves the six raw glTF `minFilter` values verbatim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IrMinFilter {
    Nearest,
    Linear,
    NearestMipmapNearest,
    LinearMipmapNearest,
    NearestMipmapLinear,
    LinearMipmapLinear,
}

/// Sampler info that corresponds to a glTF sampler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IrSamplerInfo {
    pub wrap_u: IrWrapMode,
    pub wrap_v: IrWrapMode,
    pub mag_filter: IrMagFilter,
    pub min_filter: IrMinFilter,
}

impl Default for IrSamplerInfo {
    fn default() -> Self {
        Self {
            wrap_u: IrWrapMode::Repeat,
            wrap_v: IrWrapMode::Repeat,
            mag_filter: IrMagFilter::Linear,
            min_filter: IrMinFilter::LinearMipmapLinear,
        }
    }
}

/// Enum identifying texture-assignment slots used during material editing (Section B).
///
/// `assign_texture_core` receives this as the `slot` argument; it distinguishes the eight MToon
/// auxiliary slots, the three standard slots, and the two MMD-specific slots. As of Step 1-4 only
/// `BaseColor` exercises an existing write path; the remaining slots gain write paths from Step 2 onward.
///
/// | Slot | Destination | Color space |
/// |---|---|---|
/// | `BaseColor` | `IrMaterial.texture_index` + `base_color_tex_info` | sRGB |
/// | `Emissive` | `IrMaterial.emissive_texture` | sRGB |
/// | `Normal` | `IrMaterial.normal_texture` | **Linear (Unorm view)** |
/// | `ShadeMultiply` | `MtoonParams.shade_texture` | sRGB |
/// | `ShadingShift` | `MtoonParams.shading_shift_texture` | Linear |
/// | `RimMultiply` | `MtoonParams.rim_multiply_texture` | sRGB |
/// | `OutlineWidth` | `MtoonParams.outline_width_texture` | Linear |
/// | `Matcap` | `MtoonParams.matcap_texture` | sRGB |
/// | `UvAnimMask` | `MtoonParams.uv_animation_mask_texture` | Linear |
/// | `Sphere` / `Toon` | `sphere_texture_index` / `toon_texture_index` | sRGB (MMD-only) |
///
/// `serde` derivations are added preemptively because Step 3 will use this enum as the key of
/// `TextureSlotRecord<S>` inside `MaterialEditRecord` persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextureSlot {
    BaseColor,
    Emissive,
    Normal,
    ShadeMultiply,
    ShadingShift,
    RimMultiply,
    OutlineWidth,
    Matcap,
    UvAnimMask,
    Sphere,
    Toon,
}

impl TextureSlot {
    /// When `true`, the GPU upload uses a Linear / Unorm view.
    /// Returns true for slots that must skip sRGB decoding (normal maps, UV masks, etc.).
    ///
    /// `rebuild_mtoon_aux_bind_group` and `assign_texture_core` use this to pick between
    /// sRGB / Unorm views (matches the decision table in Section B).
    pub fn is_linear(self) -> bool {
        matches!(
            self,
            Self::Normal | Self::ShadingShift | Self::OutlineWidth | Self::UvAnimMask
        )
    }
}

/// MToon auxiliary texture info (texCoord + KHR_texture_transform + sampler).
#[derive(Clone, Debug)]
pub struct IrTextureInfo {
    pub index: usize,
    /// TEXCOORD set index (default 0).
    pub tex_coord: u32,
    /// KHR_texture_transform offset (default (0, 0)).
    pub offset: Vec2,
    /// KHR_texture_transform scale (default (1, 1)).
    pub scale: Vec2,
    /// KHR_texture_transform rotation (default 0).
    pub rotation: f32,
    /// glTF sampler info (default Repeat / Linear).
    pub sampler: IrSamplerInfo,
}

impl IrTextureInfo {
    /// Construct from a texture index only (defaults: texCoord = 0, no transform).
    pub fn from_index(index: usize) -> Self {
        Self {
            index,
            tex_coord: 0,
            offset: Vec2::ZERO,
            scale: Vec2::ONE,
            rotation: 0.0,
            sampler: IrSamplerInfo::default(),
        }
    }

    /// Offset the texture index (used during merge).
    pub fn offset_index(&mut self, offset: usize) {
        self.index += offset;
    }

    /// Remap the texture index (used during export_filter).
    pub fn remap_index(self, remap: &std::collections::HashMap<usize, usize>) -> Option<Self> {
        remap.get(&self.index).map(|&new_idx| Self {
            index: new_idx,
            ..self
        })
    }
}

/// MToon outline-width mode.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub enum OutlineWidthMode {
    #[default]
    None,
    WorldCoordinates,
    ScreenCoordinates,
}

/// Alpha mode unified from glTF `alphaMode` + MToon `transparentWithZWrite`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum AlphaMode {
    /// Fully opaque (depth write on).
    #[default]
    Opaque,
    /// Per-pixel cutout via `alphaCutoff` (depth write on).
    Mask,
    /// Translucent with depth write (MToon `transparentWithZWrite = true`).
    BlendWithZWrite,
    /// Translucent without depth write (regular BLEND).
    Blend,
}

/// Source file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SourceFormat {
    #[default]
    Vrm1,
    Vrm0,
    Fbx,
    Pmx,
    Pmd,
    Obj,
    Stl,
    DirectX,
}

impl SourceFormat {
    pub fn label(&self) -> &str {
        match self {
            SourceFormat::Vrm1 => "VRM 1.0",
            SourceFormat::Vrm0 => "VRM 0.0",
            SourceFormat::Fbx => "FBX",
            SourceFormat::Pmx => "PMX",
            SourceFormat::Pmd => "PMD",
            SourceFormat::Obj => "OBJ",
            SourceFormat::Stl => "STL",
            SourceFormat::DirectX => "DirectX",
        }
    }

    /// Whether to use the VRM 0.0 coord transform.
    pub fn is_vrm0(&self) -> bool {
        matches!(self, SourceFormat::Vrm0)
    }

    /// Whether the format is PMX or PMD.
    pub fn is_pmx_pmd(&self) -> bool {
        matches!(self, SourceFormat::Pmx | SourceFormat::Pmd)
    }
}

/// Result of an A-stance conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AStanceResult {
    /// Not applied (checkbox off, or unsupported format).
    #[default]
    NotRequested,
    /// Applied successfully (number of corrected arms; typically 2).
    Applied(usize),
    /// Skipped because the model is already close to A-stance.
    AlreadyAStance,
    /// Failed because the arm bones were not found.
    NotFound,
}

/// Intermediate-representation model.
#[derive(Debug, Default, Clone)]
pub struct IrModel {
    pub name: String,
    pub comment: String,
    pub bones: Vec<IrBone>,
    pub meshes: Vec<IrMesh>,
    pub materials: Vec<IrMaterial>,
    pub textures: Vec<IrTexture>,
    pub morphs: Vec<IrMorph>,
    pub physics: IrPhysics,
    /// Node-index -> bone-index mapping.
    pub node_to_bone: std::collections::HashMap<usize, usize>,
    /// Source file format (used to branch on coord conversions).
    pub source_format: SourceFormat,
    /// Humanoid rig kind (FBX-specific; None = not detected / VRM).
    pub rig_type: Option<String>,
    /// Number of humanoid bones that were mapped.
    pub humanoid_bone_count: usize,
    /// Result of the A-stance conversion.
    pub astance_result: AStanceResult,
}

impl IrModel {
    /// Lightweight clone for PMX export. Drops GPU-only data (`mip_chain`, `uvs1`).
    /// `vertices`, `indices`, and `morph_targets` are Arc-shared for O(1) clones.
    pub fn clone_for_export(&self) -> Self {
        Self {
            name: self.name.clone(),
            comment: self.comment.clone(),
            bones: self.bones.clone(),
            meshes: self
                .meshes
                .iter()
                .map(|m| IrMesh {
                    name: m.name.clone(),
                    vertices: Arc::clone(&m.vertices),
                    indices: Arc::clone(&m.indices),
                    material_index: m.material_index,
                    morph_targets: Arc::clone(&m.morph_targets),
                    node_index: m.node_index,
                    uvs1: Vec::new(), // Unused in PMX
                })
                .collect(),
            materials: self.materials.clone(),
            textures: self
                .textures
                .iter()
                .map(|t| IrTexture {
                    filename: t.filename.clone(),
                    data: t.data.clone(), // Arc-shared, cheap to clone
                    mime_type: t.mime_type.clone(),
                    source_path: t.source_path.clone(),
                    mip_chain: None, // GPU-only; not used by PMX
                })
                .collect(),
            morphs: self.morphs.clone(),
            physics: self.physics.clone(),
            node_to_bone: self.node_to_bone.clone(),
            source_format: self.source_format,
            rig_type: self.rig_type.clone(),
            humanoid_bone_count: self.humanoid_bone_count,
            astance_result: self.astance_result,
        }
    }

    /// Total vertex count across every mesh.
    pub fn total_vertices(&self) -> usize {
        self.meshes.iter().map(|m| m.vertices.len()).sum()
    }

    /// Total face count across every mesh.
    pub fn total_faces(&self) -> usize {
        self.meshes.iter().map(|m| m.indices.len() / 3).sum()
    }

    /// Merge another `IrModel` into this one (for "additional load" operations).
    ///
    /// Bones with the same name are merged into the existing side; only unique bones are added.
    /// Texture / material / mesh / morph / physics indices are remapped via lookup tables.
    ///
    /// Returns (number of merged bones, number of newly added bones).
    pub fn merge(&mut self, mut other: IrModel) -> (usize, usize) {
        let tex_offset = self.textures.len();
        let mat_offset = self.materials.len();
        let rigid_offset = self.physics.rigid_bodies.len();
        let morph_offset = self.morphs.len();
        let vtx_offset = self.total_vertices();

        // -- Build the bone remap table --
        // Mapping other_bone_idx -> self_bone_idx
        let mut bone_name_to_self: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::with_capacity(self.bones.len());
        for (i, bone) in self.bones.iter().enumerate() {
            bone_name_to_self.insert(&bone.name, i);
        }

        let other_bone_count = other.bones.len();
        // Remap table: which self-side index each other-side bone corresponds to
        let mut bone_remap: Vec<usize> = vec![usize::MAX; other_bone_count];
        // Which other-side bones were newly added
        let mut is_new_bone: Vec<bool> = vec![true; other_bone_count];
        let mut merged_count: usize = 0;

        // -- Three-tier fallback for candidate selection --
        // candidate[i] = Some(self_idx) marks a merge candidate
        let mut candidate: Vec<Option<usize>> = vec![None; other_bone_count];
        // vrm_bone_name matches are treated as final (excluded from the parent-propagation undo in pass 2)
        let mut is_vrm_match: Vec<bool> = vec![false; other_bone_count];

        // Pass 1a: vrm_bone_name match (highest confidence; no parent check -- VRM names are body-wide unique)
        let mut vrm_to_self: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for (i, bone) in self.bones.iter().enumerate() {
            if let Some(ref vrm) = bone.vrm_bone_name {
                vrm_to_self.entry(vrm.as_str()).or_insert(i);
            }
        }
        for (i, other_bone) in other.bones.iter().enumerate() {
            if let Some(ref vrm) = other_bone.vrm_bone_name {
                if let Some(&self_idx) = vrm_to_self.get(vrm.as_str()) {
                    candidate[i] = Some(self_idx);
                    is_vrm_match[i] = true;
                }
            }
        }

        // Pass 1b: original_name match (with parent-consistency check).
        // Pre-cache `to_lowercase()` to avoid repeated allocations.
        let self_lower_names: Vec<String> = self
            .bones
            .iter()
            .map(|b| b.original_name.to_lowercase())
            .collect();
        let other_lower_names: Vec<String> = other
            .bones
            .iter()
            .map(|b| b.original_name.to_lowercase())
            .collect();
        let mut orig_to_self: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for (i, key) in self_lower_names.iter().enumerate() {
            orig_to_self.entry(key.as_str()).or_insert(i);
        }
        for (i, other_bone) in other.bones.iter().enumerate() {
            if candidate[i].is_some() {
                continue;
            }
            let key = &other_lower_names[i];
            if let Some(&self_idx) = orig_to_self.get(key.as_str()) {
                let parent_ok = match (other_bone.parent, self.bones[self_idx].parent) {
                    (None, None) => true,
                    (Some(op), Some(sp)) => {
                        candidate[op] == Some(sp) || self_lower_names[sp] == other_lower_names[op]
                    }
                    _ => false,
                };
                if parent_ok {
                    candidate[i] = Some(self_idx);
                }
            }
        }

        // Pass 1c: bone.name match (legacy logic, kept for backwards compatibility)
        for (i, other_bone) in other.bones.iter().enumerate() {
            if candidate[i].is_some() {
                continue;
            }
            if let Some(&self_idx) = bone_name_to_self.get(other_bone.name.as_str()) {
                let self_parent_name = self.bones[self_idx]
                    .parent
                    .map(|p| self.bones[p].name.as_str());
                let other_parent_name = other_bone.parent.map(|p| other.bones[p].name.as_str());
                if self_parent_name == other_parent_name {
                    candidate[i] = Some(self_idx);
                }
            }
        }

        // Pass 2: propagate the parent's merge state to converge on a final decision (order-independent).
        // If a candidate's parent is not itself a candidate, drop the merge (prevents misjoining
        // descendants of distinct subtrees).
        // vrm_bone_name matches are semantically final, so they are excluded from the undo.
        let mut changed = true;
        while changed {
            changed = false;
            for i in 0..other_bone_count {
                if candidate[i].is_none() || is_vrm_match[i] {
                    continue;
                }
                if let Some(parent_idx) = other.bones[i].parent {
                    // Parent is not a candidate -> this child also cannot be merged
                    if candidate[parent_idx].is_none() {
                        candidate[i] = None;
                        changed = true;
                    }
                }
            }
        }

        // Lock in the candidates
        for i in 0..other_bone_count {
            if let Some(self_idx) = candidate[i] {
                bone_remap[i] = self_idx;
                is_new_bone[i] = false;
                merged_count += 1;
            }
        }

        // Assign accurate indices to new bones
        let mut next_new_idx = self.bones.len();
        for i in 0..other_bone_count {
            if is_new_bone[i] {
                bone_remap[i] = next_new_idx;
                next_new_idx += 1;
            }
        }
        let new_bone_count = next_new_idx - self.bones.len();

        // Avoid node_index collisions: use (existing max node_id + 1) as the offset
        let max_existing_node = self.bones.iter().map(|b| b.node_index).max().unwrap_or(0);
        let max_mesh_node = self.meshes.iter().map(|m| m.node_index).max().unwrap_or(0);
        let node_offset = max_existing_node.max(max_mesh_node) + 1;

        // Append new bones to self.bones (remapping parent/children)
        for (other_idx, other_bone) in other.bones.iter().enumerate() {
            if !is_new_bone[other_idx] {
                // Merged into an existing bone -> extend the existing bone's `children` with
                // the appended model's children. We patch existing-to-existing connections too,
                // not just new bones.
                let self_idx = bone_remap[other_idx];
                for &child_other_idx in &other_bone.children {
                    let child_self_idx = bone_remap[child_other_idx];
                    if !self.bones[self_idx].children.contains(&child_self_idx) {
                        self.bones[self_idx].children.push(child_self_idx);
                    }
                }
                // Backfill humanoid metadata (only when the existing side is unset)
                if self.bones[self_idx].vrm_bone_name.is_none() {
                    if let Some(ref vrm_name) = other_bone.vrm_bone_name {
                        self.bones[self_idx].vrm_bone_name = Some(vrm_name.clone());
                    }
                }
                continue;
            }

            let mut new_bone = other_bone.clone();
            // Remap parent
            if let Some(ref mut p) = new_bone.parent {
                *p = bone_remap[*p];
            }
            // Remap children
            for child in &mut new_bone.children {
                *child = bone_remap[*child];
            }
            // Avoid node_index collisions (a fixed offset keeps every struct in sync)
            new_bone.node_index += node_offset;
            self.bones.push(new_bone);
        }

        // Update the node_to_bone mapping (uses the same node_offset)
        for (node, bone_idx) in other.node_to_bone {
            let remapped = bone_remap[bone_idx];
            self.node_to_bone.insert(node + node_offset, remapped);
        }

        // -- Textures --
        self.textures.append(&mut other.textures);

        // -- Materials: offset every texture index --
        for mat in &mut other.materials {
            if let Some(ref mut idx) = mat.texture_index {
                *idx += tex_offset;
            }
            if let Some(ref mut ti) = mat.base_color_tex_info {
                ti.offset_index(tex_offset);
            }
            if let Some(ref mut m) = mat.mtoon {
                if let Some(ref mut ti) = m.shade_texture {
                    ti.offset_index(tex_offset);
                }
                if let Some(ref mut ti) = m.outline_width_texture {
                    ti.offset_index(tex_offset);
                }
                if let Some(ref mut ti) = m.matcap_texture {
                    ti.offset_index(tex_offset);
                }
                if let Some(ref mut ti) = m.shading_shift_texture {
                    ti.offset_index(tex_offset);
                }
                if let Some(ref mut ti) = m.rim_multiply_texture {
                    ti.offset_index(tex_offset);
                }
                if let Some(ref mut ti) = m.uv_animation_mask_texture {
                    ti.offset_index(tex_offset);
                }
            }
            if let Some(ref mut ti) = mat.emissive_texture {
                ti.offset_index(tex_offset);
            }
            if let Some(ref mut ti) = mat.normal_texture {
                ti.offset_index(tex_offset);
            }
            if let Some(ref mut idx) = mat.sphere_texture_index {
                *idx += tex_offset;
            }
            if let Some(ref mut idx) = mat.toon_texture_index {
                *idx += tex_offset;
            }
        }
        self.materials.append(&mut other.materials);

        // -- Meshes: remap material indices and vertex-weight bone indices --
        for mesh in &mut other.meshes {
            mesh.material_index += mat_offset;
            mesh.node_index += node_offset;
            for vtx in mesh.vertices_mut() {
                for (bone_idx, _) in vtx.active_weights_mut() {
                    *bone_idx = bone_remap[*bone_idx];
                }
            }
        }
        self.meshes.append(&mut other.meshes);

        // -- Morphs: offset global vertex indices --
        for morph in &mut other.morphs {
            match &mut morph.kind {
                IrMorphKind::Vertex {
                    positions,
                    normals,
                    tangents,
                } => {
                    for (global_idx, _) in positions.iter_mut() {
                        *global_idx += vtx_offset;
                    }
                    for (global_idx, _) in normals.iter_mut() {
                        *global_idx += vtx_offset;
                    }
                    for (global_idx, _) in tangents.iter_mut() {
                        *global_idx += vtx_offset;
                    }
                }
                IrMorphKind::Group(entries) => {
                    for (morph_idx, _) in entries.iter_mut() {
                        *morph_idx += morph_offset;
                    }
                }
                IrMorphKind::Material {
                    color_binds,
                    uv_binds,
                } => {
                    for b in color_binds.iter_mut() {
                        b.material_index += mat_offset;
                    }
                    for b in uv_binds.iter_mut() {
                        b.material_index += mat_offset;
                    }
                }
                IrMorphKind::Uv {
                    channel: _,
                    offsets,
                } => {
                    // Phase 3 A-2: shift the appended model's global vertex indices by vtx_offset too.
                    for (global_idx, _) in offsets.iter_mut() {
                        *global_idx += vtx_offset;
                    }
                }
            }
        }
        self.morphs.append(&mut other.morphs);

        // -- Physics: remap bone indices and offset rigid-body indices --
        for rb in &mut other.physics.rigid_bodies {
            if let Some(ref mut idx) = rb.bone_index {
                *idx = bone_remap[*idx];
            }
        }
        self.physics
            .rigid_bodies
            .append(&mut other.physics.rigid_bodies);

        for joint in &mut other.physics.joints {
            joint.rigid_a += rigid_offset;
            joint.rigid_b += rigid_offset;
        }
        self.physics.joints.append(&mut other.physics.joints);

        // -- Update metadata --
        self.name = format!("{} + {}", self.name, other.name);
        // Recount humanoid bones (so backfilled shared bones are included)
        self.humanoid_bone_count = self
            .bones
            .iter()
            .filter(|b| b.vrm_bone_name.is_some())
            .count();
        // Combine the A-stance conversion results.
        // NotRequested is transparent; Applied wins over NotFound (matters when appending small props).
        self.astance_result = match (self.astance_result, other.astance_result) {
            // Drop NotRequested in favor of the other side
            (AStanceResult::NotRequested, other) => other,
            (host, AStanceResult::NotRequested) => host,
            // Applied + Applied -> sum
            (AStanceResult::Applied(a), AStanceResult::Applied(b)) => AStanceResult::Applied(a + b),
            // Applied + NotFound/AlreadyAStance -> Applied wins
            // (if the main model was already converted, the prop's NotFound is harmless)
            (AStanceResult::Applied(n), _) | (_, AStanceResult::Applied(n)) => {
                AStanceResult::Applied(n)
            }
            // Both NotFound
            (AStanceResult::NotFound, AStanceResult::NotFound) => AStanceResult::NotFound,
            // AlreadyAStance + NotFound -> AlreadyAStance wins
            (AStanceResult::AlreadyAStance, _) | (_, AStanceResult::AlreadyAStance) => {
                AStanceResult::AlreadyAStance
            }
        };

        (merged_count, new_bone_count)
    }

    /// Log the texture assignments of every material.
    pub fn log_texture_assignments(&self) {
        // Helper that renders "texture name + source path"
        let tex_label = |idx: usize| -> String {
            self.textures.get(idx).map_or("?".to_string(), |t| {
                if t.source_path.is_empty() {
                    format!("\"{}\"", t.filename)
                } else {
                    format!("\"{}\" ({})", t.filename, t.source_path)
                }
            })
        };
        let info_label = |info: &IrTextureInfo| -> String { tex_label(info.index) };

        log::info!("=== Texture assignments ===");
        for (i, mat) in self.materials.iter().enumerate() {
            let mut parts: Vec<String> = Vec::new();

            if let Some(idx) = mat.texture_index {
                parts.push(format!("base={}", tex_label(idx)));
            } else {
                parts.push("base=none".to_string());
            }
            if let Some(ref info) = mat.normal_texture {
                parts.push(format!("normal={}", info_label(info)));
            }
            if let Some(ref info) = mat.emissive_texture {
                parts.push(format!("emissive={}", info_label(info)));
            }
            if let Some(idx) = mat.sphere_texture_index {
                let mode = match mat.sphere_mode {
                    1 => "mul",
                    2 => "add",
                    _ => "?",
                };
                parts.push(format!("sphere={} [{}]", tex_label(idx), mode));
            }
            if let Some(idx) = mat.toon_texture_index {
                parts.push(format!("toon={}", tex_label(idx)));
            } else if let Some(shared) = mat.toon_shared_index {
                parts.push(format!("toon=shared(toon{:02}.bmp)", shared + 1));
            }

            if let Some(ref mtoon) = mat.mtoon {
                if let Some(ref info) = mtoon.shade_texture {
                    parts.push(format!("shade={}", info_label(info)));
                }
                if let Some(ref info) = mtoon.rim_multiply_texture {
                    parts.push(format!("rim={}", info_label(info)));
                }
                if let Some(ref info) = mtoon.matcap_texture {
                    parts.push(format!("matcap={}", info_label(info)));
                }
                if let Some(ref info) = mtoon.outline_width_texture {
                    parts.push(format!("outline={}", info_label(info)));
                }
                if let Some(ref info) = mtoon.shading_shift_texture {
                    parts.push(format!("shading_shift={}", info_label(info)));
                }
                if let Some(ref info) = mtoon.uv_animation_mask_texture {
                    parts.push(format!("uv_anim_mask={}", info_label(info)));
                }
            }

            log::info!("Material[{}] \"{}\": {}", i, mat.name, parts.join(", "));
        }
        log::info!(
            "=== Texture assignments done ({} materials) ===",
            self.materials.len()
        );
    }
}

/// Intermediate bone.
#[derive(Debug, Clone)]
pub struct IrBone {
    pub name: String,
    pub name_en: String,
    /// Original bone name in the source file (FBX: node name; VRM: glTF node name).
    pub original_name: String,
    /// VRM humanoid bone name (e.g. "hips", "spine").
    pub vrm_bone_name: Option<String>,
    /// Global position (glTF coords).
    pub position: Vec3,
    /// Global transform (glTF coords); used to convert collider local offsets.
    pub global_mat: Mat4,
    /// Parent bone index (None for roots).
    pub parent: Option<usize>,
    /// Child-bone indices.
    pub children: Vec<usize>,
    /// glTF node index.
    pub node_index: usize,
    /// Bone-follow / physics flag.
    pub is_physics: bool,
    /// Tail position (glTF coords; PMX/PMD only).
    /// Computed from PMX's BoneTail (offset or bone index) and represents the rest-pose tip.
    pub tail_position: Option<Vec3>,
    /// Tail-target bone index (from `BoneTail::BoneIndex`, used for dynamic follow during animation).
    pub tail_bone_index: Option<usize>,
    /// Whether the bone is inside an IK chain (registered as IK Target + Link).
    pub is_ik: bool,
    /// Whether the bone is an IK controller (PMX: BONE_FLAG_IK; PMD: bone_type == 2).
    pub is_ik_bone: bool,
    /// Whether the bone is translatable (PMX: BONE_FLAG_TRANSLATABLE; PMD: bone_type == 1).
    pub is_translatable: bool,
    /// Whether the bone has an axis lock (PMX: BONE_FLAG_AXIS_FIXED).
    pub is_axis_fixed: bool,
    /// Whether the bone is visible (PMX: BONE_FLAG_VISIBLE; PMD: bone_type != 7).
    pub is_visible: bool,
    /// Grant data (PMX rotation grant / move grant).
    pub grant: Option<IrGrant>,
}

/// Grant data (PMX rotation grant / move grant).
#[derive(Debug, Clone)]
pub struct IrGrant {
    /// Grant parent bone index.
    pub parent_index: usize,
    /// Grant ratio.
    pub ratio: f32,
    /// Whether rotation is granted.
    pub is_rotation: bool,
    /// Whether translation is granted.
    pub is_move: bool,
    /// Whether the grant is local.
    pub is_local: bool,
}

/// Intermediate mesh.
///
/// `vertices`, `indices`, and `morph_targets` are `Arc`-shared so that `clone`
/// stays O(1). When mutation is required, use the `vertices_mut()` helpers
/// (which apply `Arc::make_mut` COW internally).
#[derive(Debug, Clone)]
pub struct IrMesh {
    pub name: String,
    pub vertices: Arc<Vec<IrVertex>>,
    pub indices: Arc<Vec<u32>>,
    pub material_index: usize,
    /// Morph targets (per-vertex offsets).
    pub morph_targets: Arc<Vec<IrMorphTarget>>,
    /// glTF node index this mesh belongs to.
    pub node_index: usize,
    /// TEXCOORD_1 (secondary UV). Empty means no UV1.
    pub uvs1: Vec<[f32; 2]>,
}

impl IrMesh {
    /// COW mutable access to vertices.
    #[inline]
    pub fn vertices_mut(&mut self) -> &mut Vec<IrVertex> {
        Arc::make_mut(&mut self.vertices)
    }

    /// COW mutable access to indices.
    #[inline]
    pub fn indices_mut(&mut self) -> &mut Vec<u32> {
        Arc::make_mut(&mut self.indices)
    }

    /// COW mutable access to morph_targets.
    #[inline]
    pub fn morph_targets_mut(&mut self) -> &mut Vec<IrMorphTarget> {
        Arc::make_mut(&mut self.morph_targets)
    }
}

/// Intermediate vertex.
#[derive(Debug, Clone, Copy)]
pub struct IrVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
    /// Tangent vector (xyz = tangent direction, w = handedness +/-1).
    /// Either read from the glTF TANGENT attribute or generated with MikkTSpace.
    pub tangent: Vec4,
    /// Fixed-size bone-weight array (bone index, weight). The first `weight_count` entries are valid.
    pub weights: [(usize, f32); 4],
    /// Number of valid weights (0..=4).
    pub weight_count: u8,
    pub edge_scale: f32, // Edge multiplier (from outlineWidthMultiplyTexture).
}

impl IrVertex {
    /// Return the active slice of weights.
    #[inline]
    pub fn active_weights(&self) -> &[(usize, f32)] {
        &self.weights[..self.weight_count as usize]
    }

    /// Return the active slice of weights mutably.
    #[inline]
    pub fn active_weights_mut(&mut self) -> &mut [(usize, f32)] {
        &mut self.weights[..self.weight_count as usize]
    }

    /// Set weights from a `Vec` (cap at 4 entries; surplus is truncated).
    pub fn set_weights_from_vec(&mut self, src: &[(usize, f32)]) {
        let n = src.len().min(4);
        self.weights = [(0, 0.0); 4];
        self.weights[..n].copy_from_slice(&src[..n]);
        self.weight_count = n as u8;
    }

    /// Helper that builds the weighted form of an `IrVertex` from `Vec<(usize, f32)>`.
    pub fn from_weights(src: Vec<(usize, f32)>) -> ([(usize, f32); 4], u8) {
        let mut arr = [(0usize, 0.0f32); 4];
        let n = src.len().min(4);
        for (i, &val) in src.iter().take(4).enumerate() {
            arr[i] = val;
        }
        (arr, n as u8)
    }
}

/// Morph target (per-mesh).
#[derive(Debug, Clone)]
pub struct IrMorphTarget {
    pub name: String,
    /// Position offsets for affected vertices (sparse, sorted by vertex index ascending).
    pub position_offsets: Vec<(u32, Vec3)>,
    /// Normal offsets for affected vertices (sparse, sorted by vertex index ascending).
    pub normal_offsets: Vec<(u32, Vec3)>,
    /// Tangent offsets for affected vertices (sparse, sorted by vertex index ascending).
    pub tangent_offsets: Vec<(u32, Vec3)>,
}

/// Cull mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CullMode {
    /// Back-face culling (default; single-sided rendering).
    Back,
    /// No culling (double-sided rendering).
    None,
    /// Front-face culling (for VRM 0.x `_CullMode = 1`). Not in the glTF spec, so UniVRM falls back
    /// to `doubleSided = true`, but runtime renderers can reproduce it.
    Front,
}

/// Color channel referenced by a texture mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChannel {
    R,
    G,
    B,
}

impl ColorChannel {
    /// f32 value for GPU uniforms (0.0 = R, 1.0 = G, 2.0 = B).
    pub fn to_f32(self) -> f32 {
        match self {
            Self::R => 0.0,
            Self::G => 1.0,
            Self::B => 2.0,
        }
    }
}

/// Shader family (Phase 3: supports detecting multiple shaders).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShaderFamily {
    #[default]
    Other,
    Mtoon,
    Uts2,
    LilToon,
    Poiyomi,
}

impl std::fmt::Display for ShaderFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Other => f.write_str("-"),
            Self::Mtoon => f.write_str("MToon"),
            Self::Uts2 => f.write_str("UTS2"),
            Self::LilToon => f.write_str("lilToon"),
            Self::Poiyomi => f.write_str("Poiyomi"),
        }
    }
}

/// MToon shader-specific parameters.
#[derive(Debug, Clone)]
pub struct MtoonParams {
    /// shadeColorFactor (default [0, 0, 0]).
    pub shade_color: Option<Vec3>,
    /// shadeMultiplyTexture.
    pub shade_texture: Option<IrTextureInfo>,
    /// shadingToonyFactor (0.0..=1.0; sharpness of the shadow boundary).
    pub shading_toony_factor: f32,
    /// shadingShiftFactor (-1.0..=1.0; shadow threshold shift).
    pub shading_shift_factor: f32,
    /// shadingShiftTexture (R channel).
    pub shading_shift_texture: Option<IrTextureInfo>,
    /// shadingShiftTexture.scale (default 1.0).
    pub shading_shift_texture_scale: f32,
    /// Outline-width texture (glTF texture index).
    /// VRM 1.0: outlineWidthMultiplyTexture (G channel).
    /// VRM 0.0: _OutlineWidthTexture (R channel).
    pub outline_width_texture: Option<IrTextureInfo>,
    /// Channel referenced by outlineWidthTexture (VRM 1.0 = G, VRM 0.x = R).
    pub outline_width_tex_channel: ColorChannel,
    /// Outline-width mode (used by the viewer).
    pub outline_width_mode: OutlineWidthMode,
    /// Raw outline-width value (world = meters, screen = ratio).
    pub outline_width_factor: f32,
    /// outlineLightingMixFactor (0.0 = pure color, 1.0 = mix with light).
    pub outline_lighting_mix: f32,
    /// parametricRimColorFactor (default [0, 0, 0]).
    pub parametric_rim_color: Vec3,
    /// parametricRimFresnelPowerFactor (default 5.0).
    pub parametric_rim_fresnel_power: f32,
    /// parametricRimLiftFactor (default 0.0).
    pub parametric_rim_lift: f32,
    /// rimLightingMixFactor (0.0 = emissive, 1.0 = mix with light; default 1.0).
    pub rim_lighting_mix: f32,
    /// rimMultiplyTexture.
    pub rim_multiply_texture: Option<IrTextureInfo>,
    /// giEqualizationFactor (0.0..=1.0; GI equalization, default 0.9).
    pub gi_equalization_factor: f32,
    /// matcapFactor (default [1, 1, 1]).
    pub matcap_factor: Vec3,
    /// matcapTexture.
    pub matcap_texture: Option<IrTextureInfo>,
    /// uvAnimationScrollXSpeedFactor (default 0.0).
    pub uv_animation_scroll_x_speed: f32,
    /// uvAnimationScrollYSpeedFactor (default 0.0).
    pub uv_animation_scroll_y_speed: f32,
    /// uvAnimationRotationSpeedFactor (default 0.0).
    pub uv_animation_rotation_speed: f32,
    /// uvAnimationMaskTexture.
    pub uv_animation_mask_texture: Option<IrTextureInfo>,
    /// Channel referenced by uvAnimationMaskTexture (VRM 1.0 = B, VRM 0.x = R).
    pub uv_anim_mask_tex_channel: ColorChannel,
    /// renderQueueOffsetNumber (within-BLEND draw-order offset).
    pub render_queue_offset: i32,
}

impl Default for MtoonParams {
    fn default() -> Self {
        Self {
            shade_color: None,
            shade_texture: None,
            shading_toony_factor: 0.9,
            shading_shift_factor: 0.0,
            shading_shift_texture: None,
            shading_shift_texture_scale: 1.0,
            outline_width_texture: None,
            outline_width_tex_channel: ColorChannel::G,
            outline_width_mode: OutlineWidthMode::None,
            outline_width_factor: 0.0,
            outline_lighting_mix: 1.0,
            parametric_rim_color: Vec3::ZERO,
            parametric_rim_fresnel_power: 5.0,
            parametric_rim_lift: 0.0,
            rim_lighting_mix: 1.0,
            rim_multiply_texture: None,
            gi_equalization_factor: 0.9,
            matcap_factor: Vec3::ONE,
            matcap_texture: None,
            uv_animation_scroll_x_speed: 0.0,
            uv_animation_scroll_y_speed: 0.0,
            uv_animation_rotation_speed: 0.0,
            uv_animation_mask_texture: None,
            uv_anim_mask_tex_channel: ColorChannel::B,
            render_queue_offset: 0,
        }
    }
}

/// Empty MtoonParams constant (for field access from non-MToon materials).
static MTOON_DEFAULT: std::sync::LazyLock<MtoonParams> =
    std::sync::LazyLock::new(MtoonParams::default);

/// Source location of a material inside an FBX (renderer hierarchy path + slot number).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceMaterialRef {
    /// Renderer hierarchy path (same-name siblings receive an ordinal suffix: "Root/Body[1]").
    pub renderer_path: std::sync::Arc<str>,
    /// Material slot index inside the mesh.
    pub slot_index: u16,
}

/// Intermediate material.
#[derive(Debug, Clone)]
pub struct IrMaterial {
    pub name: String,
    pub diffuse: Vec4,
    pub specular: Vec3,
    pub specular_power: f32,
    pub ambient: Vec3,
    pub texture_index: Option<usize>,
    /// Base-color texCoord + KHR_texture_transform info (used by the viewer).
    pub base_color_tex_info: Option<IrTextureInfo>,
    /// Cull mode (Back = single-sided, None = double-sided, Front = front-face culling).
    pub cull_mode: CullMode,
    /// Edge (outline) color.
    pub edge_color: Vec4,
    pub edge_size: f32,
    /// MToon shader-specific parameters (None means non-MToon material).
    pub mtoon: Option<MtoonParams>,
    /// Shader family (MToon / UTS2 / Other).
    pub shader_family: ShaderFamily,
    /// Original texture filename in the FBX (used for batch assignment).
    pub source_texture_name: Option<String>,
    /// Material origin (used to decide RenderStyle).
    pub source_format: SourceFormat,
    /// Sphere-map texture.
    pub sphere_texture_index: Option<usize>,
    /// Sphere mode: 0 = off, 1 = multiply, 2 = add (3 = sub-texture is unsupported).
    pub sphere_mode: u8,
    /// Per-material toon texture index.
    pub toon_texture_index: Option<usize>,
    /// Shared-toon number (0-9 = toon01-10).
    pub toon_shared_index: Option<u8>,
    /// Alpha mode (glTF `alphaMode` + MToon `transparentWithZWrite`).
    pub alpha_mode: AlphaMode,
    /// Cutoff threshold in MASK mode (glTF `alphaCutoff`, default 0.5).
    pub alpha_cutoff: f32,
    /// glTF `emissiveFactor` (default [0, 0, 0]).
    pub emissive_factor: Vec3,
    /// glTF `emissiveTexture`.
    pub emissive_texture: Option<IrTextureInfo>,
    /// glTF `normalTexture`.
    pub normal_texture: Option<IrTextureInfo>,
    /// glTF `normalTexture.scale` (default 1.0).
    pub normal_texture_scale: f32,
    /// Material source location inside the FBX (used for Prefab texture mapping).
    pub source_material: Option<SourceMaterialRef>,
}

impl IrMaterial {
    /// Whether this is an MToon material.
    pub fn is_mtoon(&self) -> bool {
        self.mtoon.is_some()
    }

    /// Reference to the MToon parameters (returns the default for non-MToon materials).
    pub fn mtoon(&self) -> &MtoonParams {
        self.mtoon.as_ref().unwrap_or(&MTOON_DEFAULT)
    }

    /// Mutable reference to the MToon parameters (initialized with defaults when None).
    pub fn mtoon_mut(&mut self) -> &mut MtoonParams {
        self.mtoon.get_or_insert_with(MtoonParams::default)
    }

    /// Apply the PMX material parameter defaults that are used when a texture is present.
    pub fn apply_textured_defaults(&mut self) {
        let alpha = self.diffuse.w;
        self.diffuse = Vec4::new(1.0, 1.0, 1.0, alpha);
        self.ambient = Vec3::new(0.5, 0.5, 0.5);
        self.specular = Vec3::ZERO;
        self.specular_power = 0.0;
    }
}

impl Default for IrMaterial {
    fn default() -> Self {
        Self {
            name: String::new(),
            diffuse: Vec4::new(1.0, 1.0, 1.0, 1.0),
            specular: Vec3::ZERO,
            specular_power: 0.0,
            ambient: Vec3::new(0.4, 0.4, 0.4),
            texture_index: None,
            base_color_tex_info: None,
            cull_mode: CullMode::Back,
            edge_color: Vec4::new(0.0, 0.0, 0.0, 1.0),
            edge_size: 0.0,
            mtoon: None,
            shader_family: ShaderFamily::Other,
            source_texture_name: None,
            source_format: SourceFormat::Vrm1,
            sphere_texture_index: None,
            sphere_mode: 0,
            toon_texture_index: None,
            toon_shared_index: None,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            emissive_factor: Vec3::ZERO,
            emissive_texture: None,
            normal_texture: None,
            normal_texture_scale: 1.0,
            source_material: None,
        }
    }
}

/// Texture payload.
#[derive(Debug, Clone)]
pub enum TextureData {
    /// Encoded binary (PNG / JPEG / TGA / etc.).
    Encoded(Arc<[u8]>),
    /// Decoded raw RGBA pixels (skip decoding on GPU upload).
    /// `pixels` is `Arc`-shared so cloning an `IrModel` is cheap.
    RawRgba {
        pixels: Arc<[u8]>,
        width: u32,
        height: u32,
    },
}

impl TextureData {
    /// Return a reference to the byte buffer.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Encoded(v) => v,
            Self::RawRgba { pixels, .. } => pixels,
        }
    }

    /// Return the data length.
    pub fn len(&self) -> usize {
        self.as_bytes().len()
    }

    /// Whether the data is empty.
    pub fn is_empty(&self) -> bool {
        self.as_bytes().is_empty()
    }
}

/// Intermediate texture.
#[derive(Debug, Clone)]
#[allow(clippy::type_complexity)]
pub struct IrTexture {
    /// Filename (used during output).
    pub filename: String,
    /// Texture data (encoded or raw RGBA).
    pub data: TextureData,
    /// MIME type (format hint for encoded data, used in logs).
    pub mime_type: String,
    /// Origin path of the texture (shown in troubleshooting logs).
    /// Embedded path / archive-internal path / external file path / etc.
    pub source_path: String,
    /// Mipmap chain (downsampled RGBA from level 1 onward).
    /// Built ahead of time on a background thread so the main thread's GPU build runs faster.
    /// Vec<(width, height, RGBA bytes)> -- Arc-shared so cloning is cheap.
    pub mip_chain: Option<Vec<(u32, u32, Arc<[u8]>)>>,
}

impl IrTexture {
    /// Whether `data` is a raw RGBA byte buffer (no PNG decoding required).
    pub fn is_raw_rgba(&self) -> bool {
        matches!(self.data, TextureData::RawRgba { .. })
    }

    /// Return the dimensions when `data` is raw RGBA (backwards-compatible helper).
    pub fn raw_dims(&self) -> Option<(u32, u32)> {
        match &self.data {
            TextureData::RawRgba { width, height, .. } => Some((*width, *height)),
            _ => None,
        }
    }
}

/// Return the MIME type for an extension (expects lowercase input).
pub fn mime_for_ext(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "tga" => "image/x-tga",
        "bmp" => "image/bmp",
        "dds" => "image/vnd.ms-dds",
        _ => "image/jpeg",
    }
}

/// Intermediate morph.
#[derive(Debug, Clone)]
pub struct IrMorph {
    pub name: String,
    pub name_en: String,
    /// Panel kind (1: Eyebrow, 2: Eye, 3: Mouth, 4: Other).
    pub panel: u8,
    pub kind: IrMorphKind,
}

#[derive(Debug, Clone)]
pub enum IrMorphKind {
    /// Vertex morph: position = (global vertex index, offset); normal / tangent share the layout.
    Vertex {
        positions: Vec<(usize, Vec3)>,
        normals: Vec<(usize, Vec3)>,
        tangents: Vec<(usize, Vec3)>,
    },
    /// Group morph: (morph index, ratio).
    Group(Vec<(usize, f32)>),
    /// Material morph: VRM 1.0 Expression's `materialColorBinds` / `textureTransformBinds`.
    Material {
        color_binds: Vec<IrMaterialColorBind>,
        uv_binds: Vec<IrTextureTransformBind>,
    },
    /// UV morph (v0.5.5 / Phase 3 A-2). Covers PMX types 3-7.
    /// `channel = 0` -> UV0 (`IrVertex.uv`); `channel = 1..=4` -> additional UV1-UV4.
    /// `offsets` is `(global vertex index, [f32; 4])`. The `[f32; 4]` matches the PMX spec exactly,
    /// and only xy is used when editing UV0. Offsets with `channel >= 2` are not composed at
    /// runtime (the GPU has no UV2-UV4 attributes); we only support reading them in and writing them back.
    Uv {
        channel: u8,
        offsets: Vec<(usize, [f32; 4])>,
    },
}

/// Target property of a VRM 1.0 `materialColorBind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialColorBindType {
    /// baseColorFactor -> IrMaterial.diffuse.
    Color,
    /// emissiveFactor -> IrMaterial.emissive_factor.
    EmissionColor,
    /// shadeColorFactor -> MtoonParams.shade_color.
    ShadeColor,
    /// matcapFactor -> MtoonParams.matcap_factor.
    MatcapColor,
    /// parametricRimColorFactor -> MtoonParams.parametric_rim_color.
    RimColor,
    /// outlineColorFactor -> IrMaterial.edge_color.
    OutlineColor,
}

impl MaterialColorBindType {
    /// Parse from the `type` string of a VRM 1.0 Expression.
    pub fn from_vrm_str(s: &str) -> Option<Self> {
        match s {
            "color" => Some(Self::Color),
            "emissionColor" => Some(Self::EmissionColor),
            "shadeColor" => Some(Self::ShadeColor),
            "matcapColor" => Some(Self::MatcapColor),
            "rimColor" => Some(Self::RimColor),
            "outlineColor" => Some(Self::OutlineColor),
            _ => None,
        }
    }
}

/// VRM 1.0 Expression `materialColorBind`.
#[derive(Debug, Clone)]
pub struct IrMaterialColorBind {
    pub material_index: usize,
    pub bind_type: MaterialColorBindType,
    pub target_value: [f32; 4],
}

/// VRM 1.0 Expression `textureTransformBind`.
#[derive(Debug, Clone)]
pub struct IrTextureTransformBind {
    pub material_index: usize,
    pub scale: [f32; 2],
    pub offset: [f32; 2],
}

/// Physics info.
#[derive(Debug, Default, Clone)]
pub struct IrPhysics {
    pub rigid_bodies: Vec<IrRigidBody>,
    pub joints: Vec<IrJoint>,
}

/// Rigid body.
#[derive(Debug, Clone)]
pub struct IrRigidBody {
    pub name: String,
    pub bone_index: Option<usize>,
    pub group: u8,
    pub no_collision_mask: u16,
    pub shape: RigidShape,
    pub position: Vec3,
    pub rotation: Vec3,
    pub mass: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub restitution: f32,
    pub friction: f32,
    pub physics_mode: u8, // 0: bone-follow, 1: physics, 2: physics + bone
}

#[derive(Debug, Clone)]
pub enum RigidShape {
    Sphere { radius: f32 },
    Box { size: Vec3 },
    Capsule { radius: f32, height: f32 },
}

/// Joint.
#[derive(Debug, Clone)]
pub struct IrJoint {
    pub name: String,
    pub rigid_a: usize,
    pub rigid_b: usize,
    pub position: Vec3,
    pub rotation: Vec3,
    pub move_limit_lo: Vec3,
    pub move_limit_hi: Vec3,
    pub rot_limit_lo: Vec3,
    pub rot_limit_hi: Vec3,
    pub spring_move: Vec3,
    pub spring_rot: Vec3,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper: build a minimal bone.
    fn bone(name: &str, parent: Option<usize>, children: Vec<usize>) -> IrBone {
        IrBone {
            name: name.to_string(),
            name_en: String::new(),
            original_name: name.to_string(),
            vrm_bone_name: None,
            position: Vec3::ZERO,
            global_mat: Mat4::IDENTITY,
            parent,
            children,
            node_index: 0,
            is_physics: false,
            tail_position: None,
            tail_bone_index: None,
            is_ik: false,
            is_ik_bone: false,
            is_translatable: false,
            is_axis_fixed: false,
            is_visible: true,
            grant: None,
        }
    }

    /// Test helper: a mesh containing weighted vertices.
    fn mesh_with_weights(name: &str, mat_idx: usize, bone_indices: &[usize]) -> IrMesh {
        let vertices: Vec<IrVertex> = bone_indices
            .iter()
            .map(|&bi| IrVertex {
                position: Vec3::ZERO,
                normal: Vec3::Y,
                uv: Vec2::ZERO,
                tangent: Vec4::new(1.0, 0.0, 0.0, 1.0),
                weights: [(bi, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)],
                weight_count: 1,
                edge_scale: 1.0,
            })
            .collect();
        IrMesh {
            name: name.to_string(),
            vertices: vertices.into(),
            indices: Arc::new(vec![0, 1, 2]),
            material_index: mat_idx,
            morph_targets: Arc::new(Vec::new()),
            node_index: 0,
            uvs1: Vec::new(),
        }
    }

    #[test]
    fn test_merge_shared_bones_are_unified() {
        // Host model: Armature -> Spine -> Head
        let mut host = IrModel {
            name: "Host".into(),
            bones: vec![
                bone("Armature", None, vec![1]),
                bone("Spine", Some(0), vec![2]),
                bone("Head", Some(1), vec![]),
            ],
            meshes: vec![mesh_with_weights("body", 0, &[1, 2, 1])],
            materials: vec![IrMaterial {
                name: "mat_body".into(),
                ..Default::default()
            }],
            ..Default::default()
        };

        // Appended model: Armature -> Spine -> Ribbon (only Ribbon is new)
        let other = IrModel {
            name: "Costume".into(),
            bones: vec![
                bone("Armature", None, vec![1]),
                bone("Spine", Some(0), vec![2]),
                bone("Ribbon", Some(1), vec![]),
            ],
            meshes: vec![mesh_with_weights("costume", 0, &[1, 2, 1])],
            materials: vec![IrMaterial {
                name: "mat_costume".into(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let (merged, new) = host.merge(other);

        // Armature(0) and Spine(1) merge; Ribbon(3) is added new
        assert_eq!(merged, 2, "Armature と Spine が統合されるべき");
        assert_eq!(new, 1, "Ribbon のみ新規追加");
        assert_eq!(host.bones.len(), 4, "Host(3) + New(1) = 4");

        // Verify the bone names
        assert_eq!(host.bones[0].name, "Armature");
        assert_eq!(host.bones[1].name, "Spine");
        assert_eq!(host.bones[2].name, "Head");
        assert_eq!(host.bones[3].name, "Ribbon");

        // Ribbon's parent should point at the existing Spine(1)
        assert_eq!(host.bones[3].parent, Some(1));

        // Ribbon(3) should appear in Spine's children
        assert!(
            host.bones[1].children.contains(&3),
            "Spine の children に Ribbon(3) がない"
        );
        // Head(2) must still be present
        assert!(
            host.bones[1].children.contains(&2),
            "Spine の children に Head(2) がない"
        );

        // The clothing mesh's vertex weights must be remapped onto the existing bones
        let costume_mesh = &host.meshes[1];
        // other's Spine(idx=1) -> self's Spine(idx=1)
        assert_eq!(
            costume_mesh.vertices[0].active_weights()[0].0,
            1,
            "Spine にリマップ"
        );
        // other's Ribbon(idx=2) -> self's Ribbon(idx=3)
        assert_eq!(
            costume_mesh.vertices[1].active_weights()[0].0,
            3,
            "Ribbon にリマップ"
        );

        // Two materials in total
        assert_eq!(host.materials.len(), 2);
        assert_eq!(host.materials[1].name, "mat_costume");
        // The clothing mesh's `material_index` must be offset
        assert_eq!(costume_mesh.material_index, 1);

        // Model name
        assert_eq!(host.name, "Host + Costume");
    }

    #[test]
    fn test_merge_physics_bone_remap() {
        let mut host = IrModel {
            name: "Host".into(),
            bones: vec![
                bone("Armature", None, vec![1]),
                bone("Spine", Some(0), vec![]),
            ],
            physics: IrPhysics {
                rigid_bodies: vec![IrRigidBody {
                    name: "rb_spine".into(),
                    bone_index: Some(1),
                    group: 0,
                    no_collision_mask: 0xFFFF,
                    shape: RigidShape::Sphere { radius: 0.1 },
                    position: Vec3::ZERO,
                    rotation: Vec3::ZERO,
                    mass: 1.0,
                    linear_damping: 0.5,
                    angular_damping: 0.5,
                    restitution: 0.0,
                    friction: 0.5,
                    physics_mode: 0,
                }],
                joints: vec![],
            },
            ..Default::default()
        };

        // Append: Armature (shared) -> NewBone (new); the rigid body is bound to NewBone
        let other = IrModel {
            name: "Accessory".into(),
            bones: vec![
                bone("Armature", None, vec![1]),
                bone("NewBone", Some(0), vec![]),
            ],
            physics: IrPhysics {
                rigid_bodies: vec![IrRigidBody {
                    name: "rb_new".into(),
                    bone_index: Some(1), // other's NewBone(1)
                    group: 1,
                    no_collision_mask: 0xFFFF,
                    shape: RigidShape::Sphere { radius: 0.05 },
                    position: Vec3::ZERO,
                    rotation: Vec3::ZERO,
                    mass: 0.5,
                    linear_damping: 0.5,
                    angular_damping: 0.5,
                    restitution: 0.0,
                    friction: 0.5,
                    physics_mode: 1,
                }],
                joints: vec![IrJoint {
                    name: "joint_new".into(),
                    rigid_a: 0, // First rigid body on the other side (actually points at the host)
                    rigid_b: 0, // Rigid body on the other side (rb_new)
                    position: Vec3::ZERO,
                    rotation: Vec3::ZERO,
                    move_limit_lo: Vec3::ZERO,
                    move_limit_hi: Vec3::ZERO,
                    rot_limit_lo: Vec3::ZERO,
                    rot_limit_hi: Vec3::ZERO,
                    spring_move: Vec3::ZERO,
                    spring_rot: Vec3::ZERO,
                }],
            },
            ..Default::default()
        };

        let (merged, new) = host.merge(other);
        assert_eq!(merged, 1); // Armature
        assert_eq!(new, 1); // NewBone

        // NewBone lands at self[2]
        assert_eq!(host.bones[2].name, "NewBone");
        assert_eq!(host.bones[2].parent, Some(0)); // Armature

        // The appended rigid body's bone_index is remapped: other NewBone(1) -> self NewBone(2)
        assert_eq!(host.physics.rigid_bodies[1].bone_index, Some(2));

        // The joint's rigid_a/b are offset by +1 (the host's rigid-body count)
        assert_eq!(host.physics.joints[0].rigid_a, 1); // 0 + rigid_offset(1)
        assert_eq!(host.physics.joints[0].rigid_b, 1);
    }

    #[test]
    fn test_merge_no_shared_bones() {
        // When there are no shared bones, every bone is added new
        let mut host = IrModel {
            name: "A".into(),
            bones: vec![bone("Root_A", None, vec![])],
            ..Default::default()
        };
        let other = IrModel {
            name: "B".into(),
            bones: vec![bone("Root_B", None, vec![])],
            ..Default::default()
        };

        let (merged, new) = host.merge(other);
        assert_eq!(merged, 0);
        assert_eq!(new, 1);
        assert_eq!(host.bones.len(), 2);
        assert_eq!(host.bones[1].name, "Root_B");
    }

    #[test]
    fn test_merge_morph_vertex_index_offset() {
        let mut host = IrModel {
            name: "Host".into(),
            meshes: vec![mesh_with_weights("m", 0, &[0, 0, 0])], // 3頂点
            bones: vec![bone("Root", None, vec![])],
            materials: vec![IrMaterial::default()],
            morphs: vec![IrMorph {
                name: "smile".into(),
                name_en: String::new(),
                panel: 3,
                kind: IrMorphKind::Vertex {
                    positions: vec![(0, Vec3::Y)],
                    normals: Vec::new(),
                    tangents: Vec::new(),
                },
            }],
            ..Default::default()
        };
        let other = IrModel {
            name: "Other".into(),
            meshes: vec![mesh_with_weights("m2", 0, &[0, 0])], // 2頂点
            bones: vec![bone("Root", None, vec![])],
            materials: vec![IrMaterial::default()],
            morphs: vec![IrMorph {
                name: "blink".into(),
                name_en: String::new(),
                panel: 2,
                kind: IrMorphKind::Vertex {
                    positions: vec![(1, Vec3::X)],
                    normals: Vec::new(),
                    tangents: Vec::new(),
                },
            }],
            ..Default::default()
        };

        host.merge(other);

        // other's morph vertex indices should be shifted by +3 (host's vertex count)
        if let IrMorphKind::Vertex { ref positions, .. } = host.morphs[1].kind {
            assert_eq!(positions[0].0, 4, "vtx_offset=3, 元Index=1 → 4");
        } else {
            panic!("頂点モーフであるべき");
        }
    }

    /// Verify that `merge` shifts `base_color_tex_info.index` by the texture offset.
    #[test]
    fn test_merge_base_color_tex_info_offset() {
        let mut host = IrModel {
            name: "Host".into(),
            bones: vec![bone("Root", None, vec![])],
            meshes: vec![mesh_with_weights("m0", 0, &[0, 0, 0])],
            materials: vec![IrMaterial {
                name: "mat0".into(),
                texture_index: Some(0),
                ..Default::default()
            }],
            textures: vec![IrTexture {
                filename: "host_tex.png".into(),
                data: TextureData::Encoded(Arc::from(vec![0u8])),
                mime_type: "image/png".into(),
                source_path: String::new(),
                mip_chain: None,
            }],
            ..Default::default()
        };

        let other = IrModel {
            name: "Other".into(),
            bones: vec![bone("Root", None, vec![])],
            meshes: vec![mesh_with_weights("m1", 0, &[0, 0, 0])],
            materials: vec![IrMaterial {
                name: "mat1".into(),
                texture_index: Some(0),
                base_color_tex_info: Some(IrTextureInfo {
                    index: 0,
                    tex_coord: 1,
                    offset: Vec2::new(0.1, 0.2),
                    scale: Vec2::new(2.0, 3.0),
                    rotation: 0.5,
                    sampler: IrSamplerInfo::default(),
                }),
                ..Default::default()
            }],
            textures: vec![IrTexture {
                filename: "other_tex.png".into(),
                data: TextureData::Encoded(Arc::from(vec![1u8])),
                mime_type: "image/png".into(),
                source_path: String::new(),
                mip_chain: None,
            }],
            ..Default::default()
        };

        host.merge(other);

        // Two textures
        assert_eq!(host.textures.len(), 2);

        // other's material `texture_index`: 0 -> 1 (offset by host's texture count = 1)
        let mat1 = &host.materials[1];
        assert_eq!(mat1.texture_index, Some(1));

        // base_color_tex_info.index should also shift 0 -> 1
        let ti = mat1.base_color_tex_info.as_ref().unwrap();
        assert_eq!(
            ti.index, 1,
            "base_color_tex_info.index がオフセットされるべき"
        );
        // UV transform data must be preserved
        assert_eq!(ti.tex_coord, 1);
        assert!((ti.offset.x - 0.1).abs() < 1e-6);
        assert!((ti.scale.y - 3.0).abs() < 1e-6);
        assert!((ti.rotation - 0.5).abs() < 1e-6);
    }

    // ===== Step 7-36: TextureSlot::is_linear tests =====

    #[test]
    fn test_texture_slot_is_linear() {
        // Linear slots: Normal, ShadingShift, OutlineWidth, UvAnimMask
        assert!(TextureSlot::Normal.is_linear());
        assert!(TextureSlot::ShadingShift.is_linear());
        assert!(TextureSlot::OutlineWidth.is_linear());
        assert!(TextureSlot::UvAnimMask.is_linear());

        // sRGB slots: every other slot
        assert!(!TextureSlot::BaseColor.is_linear());
        assert!(!TextureSlot::Emissive.is_linear());
        assert!(!TextureSlot::ShadeMultiply.is_linear());
        assert!(!TextureSlot::RimMultiply.is_linear());
        assert!(!TextureSlot::Matcap.is_linear());
        assert!(!TextureSlot::Sphere.is_linear());
        assert!(!TextureSlot::Toon.is_linear());
    }

    #[test]
    fn test_texture_slot_all_variants_covered() {
        // Iterate through all 11 variants and confirm `is_linear` does not panic
        let all = [
            TextureSlot::BaseColor,
            TextureSlot::Emissive,
            TextureSlot::Normal,
            TextureSlot::ShadeMultiply,
            TextureSlot::ShadingShift,
            TextureSlot::RimMultiply,
            TextureSlot::OutlineWidth,
            TextureSlot::Matcap,
            TextureSlot::UvAnimMask,
            TextureSlot::Sphere,
            TextureSlot::Toon,
        ];
        let linear_count = all.iter().filter(|s| s.is_linear()).count();
        assert_eq!(linear_count, 4, "Linear スロットは 4 種");
        assert_eq!(all.len(), 11, "全 11 バリアント");
    }

    #[test]
    fn material_color_bind_type_from_vrm_str() {
        assert_eq!(
            MaterialColorBindType::from_vrm_str("color"),
            Some(MaterialColorBindType::Color)
        );
        assert_eq!(
            MaterialColorBindType::from_vrm_str("emissionColor"),
            Some(MaterialColorBindType::EmissionColor)
        );
        assert_eq!(
            MaterialColorBindType::from_vrm_str("shadeColor"),
            Some(MaterialColorBindType::ShadeColor)
        );
        assert_eq!(
            MaterialColorBindType::from_vrm_str("matcapColor"),
            Some(MaterialColorBindType::MatcapColor)
        );
        assert_eq!(
            MaterialColorBindType::from_vrm_str("rimColor"),
            Some(MaterialColorBindType::RimColor)
        );
        assert_eq!(
            MaterialColorBindType::from_vrm_str("outlineColor"),
            Some(MaterialColorBindType::OutlineColor)
        );
        assert_eq!(
            MaterialColorBindType::from_vrm_str("unknownType"),
            None,
            "Unknown type should return None"
        );
        assert_eq!(
            MaterialColorBindType::from_vrm_str(""),
            None,
            "Empty string should return None"
        );
    }

    #[test]
    fn merge_material_morph_offsets_material_index() {
        let mut host = IrModel::default();
        host.materials.push(IrMaterial::default());
        host.materials.push(IrMaterial::default());

        let mut guest = IrModel::default();
        guest.materials.push(IrMaterial::default());
        guest.morphs.push(IrMorph {
            name: "mat_morph".to_string(),
            name_en: "mat_morph".to_string(),
            panel: 4,
            kind: IrMorphKind::Material {
                color_binds: vec![IrMaterialColorBind {
                    material_index: 0,
                    bind_type: MaterialColorBindType::Color,
                    target_value: [1.0, 0.0, 0.0, 1.0],
                }],
                uv_binds: vec![IrTextureTransformBind {
                    material_index: 0,
                    scale: [2.0, 2.0],
                    offset: [0.5, 0.5],
                }],
            },
        });

        host.merge(guest);

        // host had 2 materials, so guest's material_index 0 should become 2
        if let IrMorphKind::Material {
            ref color_binds,
            ref uv_binds,
        } = host.morphs[0].kind
        {
            assert_eq!(color_binds[0].material_index, 2);
            assert_eq!(uv_binds[0].material_index, 2);
        } else {
            panic!("Expected Material morph kind");
        }
    }
}
