use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigType {
    Mixamo,
    MaxBiped,
    MayaHumanIK,
    VRoid,
    Unreal,
    Blender,
    Unknown,
}

impl RigType {
    pub fn label(&self) -> &str {
        match self {
            RigType::Mixamo => "Mixamo",
            RigType::MaxBiped => "3ds Max Biped",
            RigType::MayaHumanIK => "Maya HumanIK",
            RigType::VRoid => "VRoid",
            RigType::Unreal => "Unreal",
            RigType::Blender => "Blender",
            RigType::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HumanBone {
    Hips,
    Spine,
    Chest,
    UpperChest,
    Neck,
    Head,
    LeftShoulder,
    LeftUpperArm,
    LeftLowerArm,
    LeftHand,
    RightShoulder,
    RightUpperArm,
    RightLowerArm,
    RightHand,
    LeftUpperLeg,
    LeftLowerLeg,
    LeftFoot,
    LeftToes,
    RightUpperLeg,
    RightLowerLeg,
    RightFoot,
    RightToes,
    LeftThumbProximal,
    LeftThumbIntermediate,
    LeftThumbDistal,
    LeftIndexProximal,
    LeftIndexIntermediate,
    LeftIndexDistal,
    LeftMiddleProximal,
    LeftMiddleIntermediate,
    LeftMiddleDistal,
    LeftRingProximal,
    LeftRingIntermediate,
    LeftRingDistal,
    LeftLittleProximal,
    LeftLittleIntermediate,
    LeftLittleDistal,
    RightThumbProximal,
    RightThumbIntermediate,
    RightThumbDistal,
    RightIndexProximal,
    RightIndexIntermediate,
    RightIndexDistal,
    RightMiddleProximal,
    RightMiddleIntermediate,
    RightMiddleDistal,
    RightRingProximal,
    RightRingIntermediate,
    RightRingDistal,
    RightLittleProximal,
    RightLittleIntermediate,
    RightLittleDistal,
    LeftEye,
    RightEye,
    Jaw,
}

impl HumanBone {
    pub fn label(&self) -> &str {
        match self {
            HumanBone::Hips => "Hips",
            HumanBone::Spine => "Spine",
            HumanBone::Chest => "Chest",
            HumanBone::UpperChest => "UpperChest",
            HumanBone::Neck => "Neck",
            HumanBone::Head => "Head",
            HumanBone::LeftShoulder => "L.Shoulder",
            HumanBone::LeftUpperArm => "L.UpperArm",
            HumanBone::LeftLowerArm => "L.LowerArm",
            HumanBone::LeftHand => "L.Hand",
            HumanBone::RightShoulder => "R.Shoulder",
            HumanBone::RightUpperArm => "R.UpperArm",
            HumanBone::RightLowerArm => "R.LowerArm",
            HumanBone::RightHand => "R.Hand",
            HumanBone::LeftUpperLeg => "L.UpperLeg",
            HumanBone::LeftLowerLeg => "L.LowerLeg",
            HumanBone::LeftFoot => "L.Foot",
            HumanBone::LeftToes => "L.Toes",
            HumanBone::RightUpperLeg => "R.UpperLeg",
            HumanBone::RightLowerLeg => "R.LowerLeg",
            HumanBone::RightFoot => "R.Foot",
            HumanBone::RightToes => "R.Toes",
            HumanBone::LeftEye => "L.Eye",
            HumanBone::RightEye => "R.Eye",
            HumanBone::Jaw => "Jaw",
            _ => "Finger",
        }
    }

    pub fn as_vrm_name(&self) -> &str {
        match self {
            HumanBone::Hips => "hips",
            HumanBone::Spine => "spine",
            HumanBone::Chest => "chest",
            HumanBone::UpperChest => "upperChest",
            HumanBone::Neck => "neck",
            HumanBone::Head => "head",
            HumanBone::LeftShoulder => "leftShoulder",
            HumanBone::LeftUpperArm => "leftUpperArm",
            HumanBone::LeftLowerArm => "leftLowerArm",
            HumanBone::LeftHand => "leftHand",
            HumanBone::RightShoulder => "rightShoulder",
            HumanBone::RightUpperArm => "rightUpperArm",
            HumanBone::RightLowerArm => "rightLowerArm",
            HumanBone::RightHand => "rightHand",
            HumanBone::LeftUpperLeg => "leftUpperLeg",
            HumanBone::LeftLowerLeg => "leftLowerLeg",
            HumanBone::LeftFoot => "leftFoot",
            HumanBone::LeftToes => "leftToes",
            HumanBone::RightUpperLeg => "rightUpperLeg",
            HumanBone::RightLowerLeg => "rightLowerLeg",
            HumanBone::RightFoot => "rightFoot",
            HumanBone::RightToes => "rightToes",
            // Unity ThumbProximal = VRM thumbMetacarpal (1段ずれ)
            HumanBone::LeftThumbProximal => "leftThumbMetacarpal",
            HumanBone::LeftThumbIntermediate => "leftThumbProximal",
            HumanBone::LeftThumbDistal => "leftThumbDistal",
            HumanBone::LeftIndexProximal => "leftIndexProximal",
            HumanBone::LeftIndexIntermediate => "leftIndexIntermediate",
            HumanBone::LeftIndexDistal => "leftIndexDistal",
            HumanBone::LeftMiddleProximal => "leftMiddleProximal",
            HumanBone::LeftMiddleIntermediate => "leftMiddleIntermediate",
            HumanBone::LeftMiddleDistal => "leftMiddleDistal",
            HumanBone::LeftRingProximal => "leftRingProximal",
            HumanBone::LeftRingIntermediate => "leftRingIntermediate",
            HumanBone::LeftRingDistal => "leftRingDistal",
            HumanBone::LeftLittleProximal => "leftLittleProximal",
            HumanBone::LeftLittleIntermediate => "leftLittleIntermediate",
            HumanBone::LeftLittleDistal => "leftLittleDistal",
            HumanBone::RightThumbProximal => "rightThumbMetacarpal",
            HumanBone::RightThumbIntermediate => "rightThumbProximal",
            HumanBone::RightThumbDistal => "rightThumbDistal",
            HumanBone::RightIndexProximal => "rightIndexProximal",
            HumanBone::RightIndexIntermediate => "rightIndexIntermediate",
            HumanBone::RightIndexDistal => "rightIndexDistal",
            HumanBone::RightMiddleProximal => "rightMiddleProximal",
            HumanBone::RightMiddleIntermediate => "rightMiddleIntermediate",
            HumanBone::RightMiddleDistal => "rightMiddleDistal",
            HumanBone::RightRingProximal => "rightRingProximal",
            HumanBone::RightRingIntermediate => "rightRingIntermediate",
            HumanBone::RightRingDistal => "rightRingDistal",
            HumanBone::RightLittleProximal => "rightLittleProximal",
            HumanBone::RightLittleIntermediate => "rightLittleIntermediate",
            HumanBone::RightLittleDistal => "rightLittleDistal",
            HumanBone::LeftEye => "leftEye",
            HumanBone::RightEye => "rightEye",
            HumanBone::Jaw => "jaw",
        }
    }
}

pub struct HumanoidMapping {
    pub rig_type: RigType,
    pub mapping: HashMap<usize, HumanBone>,
}

impl Default for HumanoidMapping {
    fn default() -> Self {
        Self {
            rig_type: RigType::Unknown,
            mapping: HashMap::new(),
        }
    }
}

pub fn detect_humanoid(bone_names: &[(usize, &str)]) -> HumanoidMapping {
    let rig_type = detect_rig_type(bone_names);
    let table: &[(&str, HumanBone)] = match rig_type {
        RigType::Mixamo => MIXAMO_MAP,
        RigType::VRoid => VROID_MAP,
        RigType::Unreal => UNREAL_MAP,
        RigType::Blender => BLENDER_MAP,
        _ => &[],
    };

    let mut mapping = HashMap::new();
    for &(idx, name) in bone_names {
        let lower = strip_namespace_lower(name);
        let stripped = lower.replace("mixamorig:", "").replace("mixamorig_", "");

        // Blender リグ: スペース/ドット/アンダースコアを正規化してマッチ
        let normalized = if rig_type == RigType::Blender {
            stripped.replace([' ', '.'], "_")
        } else {
            stripped
        };

        for &(pattern, bone) in table {
            if normalized == pattern || lower == pattern {
                mapping.insert(idx, bone);
                break;
            }
        }
    }

    HumanoidMapping { rig_type, mapping }
}

/// "Model::Hips" → "hips" のように名前空間プレフィックスを除去して小文字化
fn strip_namespace_lower(name: &str) -> String {
    let s = if let Some(pos) = name.rfind("::") {
        &name[pos + 2..]
    } else {
        name
    };
    s.to_lowercase()
}

fn detect_rig_type(bone_names: &[(usize, &str)]) -> RigType {
    let names: Vec<String> = bone_names
        .iter()
        .map(|(_, n)| strip_namespace_lower(n))
        .collect();

    if names
        .iter()
        .any(|n| n.starts_with("mixamorig:") || n.starts_with("mixamorig_"))
    {
        return RigType::Mixamo;
    }
    if names
        .iter()
        .any(|n| n == "j_bip_c_hips" || n.starts_with("j_bip_"))
    {
        return RigType::VRoid;
    }
    if names
        .iter()
        .any(|n| n == "bip01" || n.starts_with("bip01 "))
    {
        return RigType::MaxBiped;
    }
    if names.iter().any(|n| n.starts_with("hik_")) {
        return RigType::MayaHumanIK;
    }
    if names.iter().any(|n| n == "root") && names.iter().any(|n| n == "pelvis") {
        return RigType::Unreal;
    }

    // プレフィックスなし Mixamo: "Hips" + "Spine1" + "LeftArm" が存在
    let has_hips = names.iter().any(|n| n == "hips" || n == "下半身");
    let has_spine1 = names.iter().any(|n| n == "spine1" || n == "上半身2");
    let has_leftarm = names.iter().any(|n| n == "leftarm");
    if has_hips && has_spine1 && has_leftarm {
        return RigType::Mixamo;
    }

    // Blender 汎用: "Hips" + ("Head" or "Spine") が存在
    // Head なしの部分スケルトン（衣装FBX等）も Blender と判定する
    // 日本語ボーン名（下半身/上半身/頭）も許容
    let has_head = names.iter().any(|n| n == "head" || n == "頭");
    let has_spine = names.iter().any(|n| n == "spine" || n == "上半身");
    if has_hips && (has_head || has_spine) {
        return RigType::Blender;
    }

    RigType::Unknown
}

const MIXAMO_MAP: &[(&str, HumanBone)] = &[
    ("hips", HumanBone::Hips),
    ("spine", HumanBone::Spine),
    ("spine1", HumanBone::Chest),
    ("spine2", HumanBone::UpperChest),
    ("neck", HumanBone::Neck),
    ("head", HumanBone::Head),
    ("leftshoulder", HumanBone::LeftShoulder),
    ("leftarm", HumanBone::LeftUpperArm),
    ("leftforearm", HumanBone::LeftLowerArm),
    ("lefthand", HumanBone::LeftHand),
    ("rightshoulder", HumanBone::RightShoulder),
    ("rightarm", HumanBone::RightUpperArm),
    ("rightforearm", HumanBone::RightLowerArm),
    ("righthand", HumanBone::RightHand),
    ("leftupleg", HumanBone::LeftUpperLeg),
    ("leftleg", HumanBone::LeftLowerLeg),
    ("leftfoot", HumanBone::LeftFoot),
    ("lefttoebase", HumanBone::LeftToes),
    ("rightupleg", HumanBone::RightUpperLeg),
    ("rightleg", HumanBone::RightLowerLeg),
    ("rightfoot", HumanBone::RightFoot),
    ("righttoebase", HumanBone::RightToes),
    ("lefthandthumb1", HumanBone::LeftThumbProximal),
    ("lefthandthumb2", HumanBone::LeftThumbIntermediate),
    ("lefthandthumb3", HumanBone::LeftThumbDistal),
    ("lefthandindex1", HumanBone::LeftIndexProximal),
    ("lefthandindex2", HumanBone::LeftIndexIntermediate),
    ("lefthandindex3", HumanBone::LeftIndexDistal),
    ("lefthandmiddle1", HumanBone::LeftMiddleProximal),
    ("lefthandmiddle2", HumanBone::LeftMiddleIntermediate),
    ("lefthandmiddle3", HumanBone::LeftMiddleDistal),
    ("lefthandring1", HumanBone::LeftRingProximal),
    ("lefthandring2", HumanBone::LeftRingIntermediate),
    ("lefthandring3", HumanBone::LeftRingDistal),
    ("lefthandpinky1", HumanBone::LeftLittleProximal),
    ("lefthandpinky2", HumanBone::LeftLittleIntermediate),
    ("lefthandpinky3", HumanBone::LeftLittleDistal),
    ("righthandthumb1", HumanBone::RightThumbProximal),
    ("righthandthumb2", HumanBone::RightThumbIntermediate),
    ("righthandthumb3", HumanBone::RightThumbDistal),
    ("righthandindex1", HumanBone::RightIndexProximal),
    ("righthandindex2", HumanBone::RightIndexIntermediate),
    ("righthandindex3", HumanBone::RightIndexDistal),
    ("righthandmiddle1", HumanBone::RightMiddleProximal),
    ("righthandmiddle2", HumanBone::RightMiddleIntermediate),
    ("righthandmiddle3", HumanBone::RightMiddleDistal),
    ("righthandring1", HumanBone::RightRingProximal),
    ("righthandring2", HumanBone::RightRingIntermediate),
    ("righthandring3", HumanBone::RightRingDistal),
    ("righthandpinky1", HumanBone::RightLittleProximal),
    ("righthandpinky2", HumanBone::RightLittleIntermediate),
    ("righthandpinky3", HumanBone::RightLittleDistal),
    ("lefteye", HumanBone::LeftEye),
    ("righteye", HumanBone::RightEye),
];

const VROID_MAP: &[(&str, HumanBone)] = &[
    ("j_bip_c_hips", HumanBone::Hips),
    ("j_bip_c_spine", HumanBone::Spine),
    ("j_bip_c_chest", HumanBone::Chest),
    ("j_bip_c_upperchest", HumanBone::UpperChest),
    ("j_bip_c_neck", HumanBone::Neck),
    ("j_bip_c_head", HumanBone::Head),
    ("j_bip_l_shoulder", HumanBone::LeftShoulder),
    ("j_bip_l_upperarm", HumanBone::LeftUpperArm),
    ("j_bip_l_lowerarm", HumanBone::LeftLowerArm),
    ("j_bip_l_hand", HumanBone::LeftHand),
    ("j_bip_r_shoulder", HumanBone::RightShoulder),
    ("j_bip_r_upperarm", HumanBone::RightUpperArm),
    ("j_bip_r_lowerarm", HumanBone::RightLowerArm),
    ("j_bip_r_hand", HumanBone::RightHand),
    ("j_bip_l_upperleg", HumanBone::LeftUpperLeg),
    ("j_bip_l_lowerleg", HumanBone::LeftLowerLeg),
    ("j_bip_l_foot", HumanBone::LeftFoot),
    ("j_bip_l_toebase", HumanBone::LeftToes),
    ("j_bip_r_upperleg", HumanBone::RightUpperLeg),
    ("j_bip_r_lowerleg", HumanBone::RightLowerLeg),
    ("j_bip_r_foot", HumanBone::RightFoot),
    ("j_bip_r_toebase", HumanBone::RightToes),
];

/// Blender 汎用ボーン名（スペース/ドット/アンダースコアは "_" に正規化済み）
/// Mixamo 風英語名・日本語ボーン名も含む（混合リグ FBX 対応）
const BLENDER_MAP: &[(&str, HumanBone)] = &[
    ("hips", HumanBone::Hips),
    ("spine", HumanBone::Spine),
    ("chest", HumanBone::Chest),
    ("upper_chest", HumanBone::UpperChest),
    ("upperchest", HumanBone::UpperChest),
    ("neck", HumanBone::Neck),
    ("head", HumanBone::Head),
    ("shoulder_l", HumanBone::LeftShoulder),
    ("shoulder_r", HumanBone::RightShoulder),
    ("upper_arm_l", HumanBone::LeftUpperArm),
    ("upper_arm_r", HumanBone::RightUpperArm),
    ("upperarm_l", HumanBone::LeftUpperArm),
    ("upperarm_r", HumanBone::RightUpperArm),
    ("lower_arm_l", HumanBone::LeftLowerArm),
    ("lower_arm_r", HumanBone::RightLowerArm),
    ("lowerarm_l", HumanBone::LeftLowerArm),
    ("lowerarm_r", HumanBone::RightLowerArm),
    ("hand_l", HumanBone::LeftHand),
    ("hand_r", HumanBone::RightHand),
    ("upper_leg_l", HumanBone::LeftUpperLeg),
    ("upper_leg_r", HumanBone::RightUpperLeg),
    ("upperleg_l", HumanBone::LeftUpperLeg),
    ("upperleg_r", HumanBone::RightUpperLeg),
    ("lower_leg_l", HumanBone::LeftLowerLeg),
    ("lower_leg_r", HumanBone::RightLowerLeg),
    ("lowerleg_l", HumanBone::LeftLowerLeg),
    ("lowerleg_r", HumanBone::RightLowerLeg),
    ("foot_l", HumanBone::LeftFoot),
    ("foot_r", HumanBone::RightFoot),
    ("toes_l", HumanBone::LeftToes),
    ("toes_r", HumanBone::RightToes),
    ("toe_l", HumanBone::LeftToes),
    ("toe_r", HumanBone::RightToes),
    ("lefteye", HumanBone::LeftEye),
    ("righteye", HumanBone::RightEye),
    // --- Mixamo 風英語名（プレフィックスなし・日英混合リグ対応） ---
    ("leftshoulder", HumanBone::LeftShoulder),
    ("rightshoulder", HumanBone::RightShoulder),
    ("leftarm", HumanBone::LeftUpperArm),
    ("rightarm", HumanBone::RightUpperArm),
    ("leftforearm", HumanBone::LeftLowerArm),
    ("rightforearm", HumanBone::RightLowerArm),
    ("lefthand", HumanBone::LeftHand),
    ("righthand", HumanBone::RightHand),
    ("leftupleg", HumanBone::LeftUpperLeg),
    ("rightupleg", HumanBone::RightUpperLeg),
    ("leftleg", HumanBone::LeftLowerLeg),
    ("rightleg", HumanBone::RightLowerLeg),
    ("leftfoot", HumanBone::LeftFoot),
    ("rightfoot", HumanBone::RightFoot),
    ("lefttoebase", HumanBone::LeftToes),
    ("righttoebase", HumanBone::RightToes),
    ("lefthandthumb1", HumanBone::LeftThumbProximal),
    ("lefthandthumb2", HumanBone::LeftThumbIntermediate),
    ("lefthandthumb3", HumanBone::LeftThumbDistal),
    ("lefthandindex1", HumanBone::LeftIndexProximal),
    ("lefthandindex2", HumanBone::LeftIndexIntermediate),
    ("lefthandindex3", HumanBone::LeftIndexDistal),
    ("lefthandmiddle1", HumanBone::LeftMiddleProximal),
    ("lefthandmiddle2", HumanBone::LeftMiddleIntermediate),
    ("lefthandmiddle3", HumanBone::LeftMiddleDistal),
    ("lefthandring1", HumanBone::LeftRingProximal),
    ("lefthandring2", HumanBone::LeftRingIntermediate),
    ("lefthandring3", HumanBone::LeftRingDistal),
    ("lefthandpinky1", HumanBone::LeftLittleProximal),
    ("lefthandpinky2", HumanBone::LeftLittleIntermediate),
    ("lefthandpinky3", HumanBone::LeftLittleDistal),
    ("righthandthumb1", HumanBone::RightThumbProximal),
    ("righthandthumb2", HumanBone::RightThumbIntermediate),
    ("righthandthumb3", HumanBone::RightThumbDistal),
    ("righthandindex1", HumanBone::RightIndexProximal),
    ("righthandindex2", HumanBone::RightIndexIntermediate),
    ("righthandindex3", HumanBone::RightIndexDistal),
    ("righthandmiddle1", HumanBone::RightMiddleProximal),
    ("righthandmiddle2", HumanBone::RightMiddleIntermediate),
    ("righthandmiddle3", HumanBone::RightMiddleDistal),
    ("righthandring1", HumanBone::RightRingProximal),
    ("righthandring2", HumanBone::RightRingIntermediate),
    ("righthandring3", HumanBone::RightRingDistal),
    ("righthandpinky1", HumanBone::RightLittleProximal),
    ("righthandpinky2", HumanBone::RightLittleIntermediate),
    ("righthandpinky3", HumanBone::RightLittleDistal),
    // --- 日本語ボーン名（PMX/MMD 慣習） ---
    ("下半身", HumanBone::Hips),
    ("上半身", HumanBone::Spine),
    ("上半身2", HumanBone::Chest),
    ("上半身3", HumanBone::UpperChest),
    ("首", HumanBone::Neck),
    ("頭", HumanBone::Head),
    ("左目", HumanBone::LeftEye),
    ("右目", HumanBone::RightEye),
    ("thumb_proximal_l", HumanBone::LeftThumbProximal),
    ("thumb_proximal_r", HumanBone::RightThumbProximal),
    ("thumb_intermediate_l", HumanBone::LeftThumbIntermediate),
    ("thumb_intermediate_r", HumanBone::RightThumbIntermediate),
    ("thumb_distal_l", HumanBone::LeftThumbDistal),
    ("thumb_distal_r", HumanBone::RightThumbDistal),
    ("proximal_thumb_l", HumanBone::LeftThumbProximal),
    ("proximal_thumb_r", HumanBone::RightThumbProximal),
    ("intermediate_thumb_l", HumanBone::LeftThumbIntermediate),
    ("intermediate_thumb_r", HumanBone::RightThumbIntermediate),
    ("distal_thumb_l", HumanBone::LeftThumbDistal),
    ("distal_thumb_r", HumanBone::RightThumbDistal),
    ("proximal_index_l", HumanBone::LeftIndexProximal),
    ("proximal_index_r", HumanBone::RightIndexProximal),
    ("intermediate_index_l", HumanBone::LeftIndexIntermediate),
    ("intermediate_index_r", HumanBone::RightIndexIntermediate),
    ("distal_index_l", HumanBone::LeftIndexDistal),
    ("distal_index_r", HumanBone::RightIndexDistal),
    ("proximal_middle_l", HumanBone::LeftMiddleProximal),
    ("proximal_middle_r", HumanBone::RightMiddleProximal),
    ("intermediate_middle_l", HumanBone::LeftMiddleIntermediate),
    ("intermediate_middle_r", HumanBone::RightMiddleIntermediate),
    ("distal_middle_l", HumanBone::LeftMiddleDistal),
    ("distal_middle_r", HumanBone::RightMiddleDistal),
    ("proximal_ring_l", HumanBone::LeftRingProximal),
    ("proximal_ring_r", HumanBone::RightRingProximal),
    ("intermediate_ring_l", HumanBone::LeftRingIntermediate),
    ("intermediate_ring_r", HumanBone::RightRingIntermediate),
    ("distal_ring_l", HumanBone::LeftRingDistal),
    ("distal_ring_r", HumanBone::RightRingDistal),
    ("proximal_little_l", HumanBone::LeftLittleProximal),
    ("proximal_little_r", HumanBone::RightLittleProximal),
    ("intermediate_little_l", HumanBone::LeftLittleIntermediate),
    ("intermediate_little_r", HumanBone::RightLittleIntermediate),
    ("distal_little_l", HumanBone::LeftLittleDistal),
    ("distal_little_r", HumanBone::RightLittleDistal),
    // 逆順パターン: "Finger Position.Side" → "finger_position_side"
    ("index_proximal_l", HumanBone::LeftIndexProximal),
    ("index_proximal_r", HumanBone::RightIndexProximal),
    ("index_intermediate_l", HumanBone::LeftIndexIntermediate),
    ("index_intermediate_r", HumanBone::RightIndexIntermediate),
    ("index_distal_l", HumanBone::LeftIndexDistal),
    ("index_distal_r", HumanBone::RightIndexDistal),
    ("middle_proximal_l", HumanBone::LeftMiddleProximal),
    ("middle_proximal_r", HumanBone::RightMiddleProximal),
    ("middle_intermediate_l", HumanBone::LeftMiddleIntermediate),
    ("middle_intermediate_r", HumanBone::RightMiddleIntermediate),
    ("middle_distal_l", HumanBone::LeftMiddleDistal),
    ("middle_distal_r", HumanBone::RightMiddleDistal),
    ("ring_proximal_l", HumanBone::LeftRingProximal),
    ("ring_proximal_r", HumanBone::RightRingProximal),
    ("ring_intermediate_l", HumanBone::LeftRingIntermediate),
    ("ring_intermediate_r", HumanBone::RightRingIntermediate),
    ("ring_distal_l", HumanBone::LeftRingDistal),
    ("ring_distal_r", HumanBone::RightRingDistal),
    ("little_proximal_l", HumanBone::LeftLittleProximal),
    ("little_proximal_r", HumanBone::RightLittleProximal),
    ("little_intermediate_l", HumanBone::LeftLittleIntermediate),
    ("little_intermediate_r", HumanBone::RightLittleIntermediate),
    ("little_distal_l", HumanBone::LeftLittleDistal),
    ("little_distal_r", HumanBone::RightLittleDistal),
    ("pinky_proximal_l", HumanBone::LeftLittleProximal),
    ("pinky_proximal_r", HumanBone::RightLittleProximal),
    ("pinky_intermediate_l", HumanBone::LeftLittleIntermediate),
    ("pinky_intermediate_r", HumanBone::RightLittleIntermediate),
    ("pinky_distal_l", HumanBone::LeftLittleDistal),
    ("pinky_distal_r", HumanBone::RightLittleDistal),
];

const UNREAL_MAP: &[(&str, HumanBone)] = &[
    ("pelvis", HumanBone::Hips),
    ("spine_01", HumanBone::Spine),
    ("spine_02", HumanBone::Chest),
    ("spine_03", HumanBone::UpperChest),
    ("neck_01", HumanBone::Neck),
    ("head", HumanBone::Head),
    ("clavicle_l", HumanBone::LeftShoulder),
    ("upperarm_l", HumanBone::LeftUpperArm),
    ("lowerarm_l", HumanBone::LeftLowerArm),
    ("hand_l", HumanBone::LeftHand),
    ("clavicle_r", HumanBone::RightShoulder),
    ("upperarm_r", HumanBone::RightUpperArm),
    ("lowerarm_r", HumanBone::RightLowerArm),
    ("hand_r", HumanBone::RightHand),
    ("thigh_l", HumanBone::LeftUpperLeg),
    ("calf_l", HumanBone::LeftLowerLeg),
    ("foot_l", HumanBone::LeftFoot),
    ("ball_l", HumanBone::LeftToes),
    ("thigh_r", HumanBone::RightUpperLeg),
    ("calf_r", HumanBone::RightLowerLeg),
    ("foot_r", HumanBone::RightFoot),
    ("ball_r", HumanBone::RightToes),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn names<'a>(list: &'a [&'a str]) -> Vec<(usize, &'a str)> {
        list.iter().enumerate().map(|(i, n)| (i, *n)).collect()
    }

    #[test]
    fn test_blender_with_head() {
        let n = names(&["Armature", "Hips", "Spine", "Head"]);
        let m = detect_humanoid(&n);
        assert_eq!(m.rig_type, RigType::Blender);
        assert!(m.mapping.values().any(|h| *h == HumanBone::Hips));
    }

    #[test]
    fn test_blender_without_head() {
        // 衣装FBXなど Head なしの部分スケルトン
        let n = names(&["Armature", "Hips", "Spine", "Neck"]);
        let m = detect_humanoid(&n);
        assert_eq!(m.rig_type, RigType::Blender);
        assert!(m.mapping.values().any(|h| *h == HumanBone::Hips));
    }

    #[test]
    fn test_unknown_no_hips_no_spine() {
        let n = names(&["Armature", "Bone1", "Bone2"]);
        let m = detect_humanoid(&n);
        assert_eq!(m.rig_type, RigType::Unknown);
    }
}
