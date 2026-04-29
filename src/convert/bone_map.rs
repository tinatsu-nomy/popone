/// PMX Japanese bone name -> VRM humanoid bone name (reverse lookup).
pub fn pmx_name_to_vrm_bone(pmx_name: &str) -> Option<&'static str> {
    // Reverse lookup table for `vrm_bone_to_pmx_name`
    Some(match pmx_name {
        "センター" => "hips",
        "上半身" => "spine",
        "上半身2" => "chest",
        "上半身3" => "upperChest",
        "首" => "neck",
        "頭" => "head",
        "左目" => "leftEye",
        "右目" => "rightEye",
        "顎" => "jaw",
        "左肩" => "leftShoulder",
        "左腕" => "leftUpperArm",
        "左ひじ" => "leftLowerArm",
        "左手首" => "leftHand",
        "右肩" => "rightShoulder",
        "右腕" => "rightUpperArm",
        "右ひじ" => "rightLowerArm",
        "右手首" => "rightHand",
        "左足" => "leftUpperLeg",
        "左ひざ" => "leftLowerLeg",
        "左足首" => "leftFoot",
        "左つま先" => "leftToes",
        "右足" => "rightUpperLeg",
        "右ひざ" => "rightLowerLeg",
        "右足首" => "rightFoot",
        "右つま先" => "rightToes",
        "左親指０" => "leftThumbMetacarpal",
        "左親指１" => "leftThumbProximal",
        "左親指２" => "leftThumbDistal",
        "左人差指１" => "leftIndexProximal",
        "左人差指２" => "leftIndexIntermediate",
        "左人差指３" => "leftIndexDistal",
        "左中指１" => "leftMiddleProximal",
        "左中指２" => "leftMiddleIntermediate",
        "左中指３" => "leftMiddleDistal",
        "左薬指１" => "leftRingProximal",
        "左薬指２" => "leftRingIntermediate",
        "左薬指３" => "leftRingDistal",
        "左小指１" => "leftLittleProximal",
        "左小指２" => "leftLittleIntermediate",
        "左小指３" => "leftLittleDistal",
        "右親指０" => "rightThumbMetacarpal",
        "右親指１" => "rightThumbProximal",
        "右親指２" => "rightThumbDistal",
        "右人差指１" => "rightIndexProximal",
        "右人差指２" => "rightIndexIntermediate",
        "右人差指３" => "rightIndexDistal",
        "右中指１" => "rightMiddleProximal",
        "右中指２" => "rightMiddleIntermediate",
        "右中指３" => "rightMiddleDistal",
        "右薬指１" => "rightRingProximal",
        "右薬指２" => "rightRingIntermediate",
        "右薬指３" => "rightRingDistal",
        "右小指１" => "rightLittleProximal",
        "右小指２" => "rightLittleIntermediate",
        "右小指３" => "rightLittleDistal",
        _ => return None,
    })
}

/// VRM bone name -> (PMX Japanese name, PMX English name).
pub fn vrm_bone_to_pmx_name(vrm_name: &str) -> Option<(&'static str, &'static str)> {
    Some(match vrm_name {
        "hips" => ("下半身", "lower body"),
        "spine" => ("上半身", "upper body"),
        "chest" => ("上半身2", "upper body2"),
        "upperChest" => ("上半身3", "upper body3"),
        "neck" => ("首", "neck"),
        "head" => ("頭", "head"),
        "leftEye" => ("左目", "eye_L"),
        "rightEye" => ("右目", "eye_R"),
        "jaw" => ("顎", "jaw"),
        // Left arm
        "leftShoulder" => ("左肩", "shoulder_L"),
        "leftUpperArm" => ("左腕", "arm_L"),
        "leftLowerArm" => ("左ひじ", "elbow_L"),
        "leftHand" => ("左手首", "wrist_L"),
        // Right arm
        "rightShoulder" => ("右肩", "shoulder_R"),
        "rightUpperArm" => ("右腕", "arm_R"),
        "rightLowerArm" => ("右ひじ", "elbow_R"),
        "rightHand" => ("右手首", "wrist_R"),
        // Left leg
        "leftUpperLeg" => ("左足", "leg_L"),
        "leftLowerLeg" => ("左ひざ", "knee_L"),
        "leftFoot" => ("左足首", "ankle_L"),
        "leftToes" => ("左つま先", "toe_L"),
        // Right leg
        "rightUpperLeg" => ("右足", "leg_R"),
        "rightLowerLeg" => ("右ひざ", "knee_R"),
        "rightFoot" => ("右足首", "ankle_R"),
        "rightToes" => ("右つま先", "toe_R"),
        // Left fingers
        "leftThumbMetacarpal" => ("左親指０", "thumb0_L"),
        "leftThumbProximal" => ("左親指１", "thumb1_L"),
        "leftThumbDistal" => ("左親指２", "thumb2_L"),
        "leftIndexProximal" => ("左人差指１", "index1_L"),
        "leftIndexIntermediate" => ("左人差指２", "index2_L"),
        "leftIndexDistal" => ("左人差指３", "index3_L"),
        "leftMiddleProximal" => ("左中指１", "middle1_L"),
        "leftMiddleIntermediate" => ("左中指２", "middle2_L"),
        "leftMiddleDistal" => ("左中指３", "middle3_L"),
        "leftRingProximal" => ("左薬指１", "ring1_L"),
        "leftRingIntermediate" => ("左薬指２", "ring2_L"),
        "leftRingDistal" => ("左薬指３", "ring3_L"),
        "leftLittleProximal" => ("左小指１", "little1_L"),
        "leftLittleIntermediate" => ("左小指２", "little2_L"),
        "leftLittleDistal" => ("左小指３", "little3_L"),
        // Right fingers
        "rightThumbMetacarpal" => ("右親指０", "thumb0_R"),
        "rightThumbProximal" => ("右親指１", "thumb1_R"),
        "rightThumbDistal" => ("右親指２", "thumb2_R"),
        "rightIndexProximal" => ("右人差指１", "index1_R"),
        "rightIndexIntermediate" => ("右人差指２", "index2_R"),
        "rightIndexDistal" => ("右人差指３", "index3_R"),
        "rightMiddleProximal" => ("右中指１", "middle1_R"),
        "rightMiddleIntermediate" => ("右中指２", "middle2_R"),
        "rightMiddleDistal" => ("右中指３", "middle3_R"),
        "rightRingProximal" => ("右薬指１", "ring1_R"),
        "rightRingIntermediate" => ("右薬指２", "ring2_R"),
        "rightRingDistal" => ("右薬指３", "ring3_R"),
        "rightLittleProximal" => ("右小指１", "little1_R"),
        "rightLittleIntermediate" => ("右小指２", "little2_R"),
        "rightLittleDistal" => ("右小指３", "little3_R"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vrm_to_pmx_basic_bones() {
        // Verify basic bone conversions
        assert_eq!(vrm_bone_to_pmx_name("hips"), Some(("下半身", "lower body")));
        assert_eq!(vrm_bone_to_pmx_name("head"), Some(("頭", "head")));
        assert_eq!(
            vrm_bone_to_pmx_name("leftHand"),
            Some(("左手首", "wrist_L"))
        );
        assert_eq!(
            vrm_bone_to_pmx_name("rightFoot"),
            Some(("右足首", "ankle_R"))
        );
    }

    #[test]
    fn test_vrm_to_pmx_unknown_returns_none() {
        assert_eq!(vrm_bone_to_pmx_name("nonexistent"), None);
        assert_eq!(vrm_bone_to_pmx_name(""), None);
    }

    #[test]
    fn test_pmx_to_vrm_basic_bones() {
        // Reverse lookup uses "センター" => "hips" (not "下半身")
        assert_eq!(pmx_name_to_vrm_bone("センター"), Some("hips"));
        assert_eq!(pmx_name_to_vrm_bone("頭"), Some("head"));
        assert_eq!(pmx_name_to_vrm_bone("左手首"), Some("leftHand"));
        assert_eq!(pmx_name_to_vrm_bone("右足首"), Some("rightFoot"));
    }

    #[test]
    fn test_pmx_to_vrm_unknown_returns_none() {
        assert_eq!(pmx_name_to_vrm_bone("全ての親"), None);
        assert_eq!(pmx_name_to_vrm_bone(""), None);
    }

    #[test]
    fn test_roundtrip_vrm_pmx_vrm() {
        // VRM -> PMX -> VRM roundtrip
        // Note: "hips" is excluded because forward = "下半身" but reverse = "センター" -> "hips"
        let vrm_names = [
            "spine",
            "chest",
            "neck",
            "head",
            "leftShoulder",
            "leftUpperArm",
            "leftLowerArm",
            "leftHand",
            "rightShoulder",
            "rightUpperArm",
            "rightLowerArm",
            "rightHand",
            "leftUpperLeg",
            "leftLowerLeg",
            "leftFoot",
            "leftToes",
            "rightUpperLeg",
            "rightLowerLeg",
            "rightFoot",
            "rightToes",
        ];
        for &vrm_name in &vrm_names {
            let (pmx_jp, _) = vrm_bone_to_pmx_name(vrm_name).unwrap();
            let back = pmx_name_to_vrm_bone(pmx_jp).unwrap();
            assert_eq!(
                back, vrm_name,
                "Roundtrip failed for {vrm_name} → {pmx_jp} → {back}"
            );
        }
        // "hips" is special: forward "下半身", reverse "センター" -> "hips"
        assert_eq!(pmx_name_to_vrm_bone("センター"), Some("hips"));
        assert_eq!(vrm_bone_to_pmx_name("hips"), Some(("下半身", "lower body")));
    }

    #[test]
    fn test_all_finger_bones_mapped() {
        // Verify all finger bones are mapped.
        // Thumb: 3 joints (Metacarpal/Proximal/Distal); no Intermediate.
        // Other 4 fingers: 3 joints each (Proximal/Intermediate/Distal).
        // Total: sides * (Thumb 3 + 4 fingers * 3) = 2 * 15 = 30
        let fingers = ["Thumb", "Index", "Middle", "Ring", "Little"];
        let mut count = 0;
        for side in ["left", "right"] {
            for finger in &fingers {
                if *finger == "Thumb" {
                    // Thumb has Metacarpal/Proximal/Distal
                    for joint in &["Metacarpal", "Proximal", "Distal"] {
                        let name = format!("{side}{finger}{joint}");
                        assert!(vrm_bone_to_pmx_name(&name).is_some(), "Missing: {name}");
                        count += 1;
                    }
                } else {
                    // The other 4 fingers have Proximal/Intermediate/Distal
                    for joint in &["Proximal", "Intermediate", "Distal"] {
                        let name = format!("{side}{finger}{joint}");
                        assert!(vrm_bone_to_pmx_name(&name).is_some(), "Missing: {name}");
                        count += 1;
                    }
                }
            }
        }
        assert_eq!(count, 30, "Expected 30 finger bone mappings");
    }
}
