/// VRM 0.0 preset name -> (PMX Japanese name, panel)
pub fn preset_to_jp_v0(preset: &str) -> (String, u8) {
    let (name, panel) = match preset.to_lowercase().as_str() {
        "a" => ("あ", 3),
        "i" => ("い", 3),
        "u" => ("う", 3),
        "e" => ("え", 3),
        "o" => ("お", 3),
        "blink" => ("まばたき", 2),
        "blink_l" => ("ウィンク", 2),
        "blink_r" => ("ウィンク右", 2),
        "joy" => ("喜び", 1),
        "angry" => ("怒り", 1),
        "sorrow" => ("悲しむ", 1),
        "fun" => ("楽しい", 1),
        "neutral" => ("ニュートラル", 4),
        "lookup" => ("上", 4),
        "lookdown" => ("下", 4),
        "lookleft" => ("左", 4),
        "lookright" => ("右", 4),
        _ => return (preset.to_string(), 4),
    };
    (name.to_string(), panel)
}

/// VRM 1.0 preset name -> (PMX Japanese name, panel)
pub fn preset_to_jp_v1(preset: &str) -> (String, u8) {
    let (name, panel) = match preset {
        "aa" => ("あ", 3),
        "ih" => ("い", 3),
        "ou" => ("う", 3),
        "ee" => ("え", 3),
        "oh" => ("お", 3),
        "blink" => ("まばたき", 2),
        "blinkLeft" => ("ウィンク", 2),
        "blinkRight" => ("ウィンク右", 2),
        "happy" => ("喜び", 1),
        "angry" => ("怒り", 1),
        "sad" => ("悲しむ", 1),
        "relaxed" => ("楽しい", 1),
        "surprised" => ("驚き", 1),
        "neutral" => ("ニュートラル", 4),
        "lookUp" => ("上", 4),
        "lookDown" => ("下", 4),
        "lookLeft" => ("左", 4),
        "lookRight" => ("右", 4),
        _ => return (preset.to_string(), 4),
    };
    (name.to_string(), panel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v0_lip_sync_presets() {
        assert_eq!(preset_to_jp_v0("a"), ("あ".to_string(), 3));
        assert_eq!(preset_to_jp_v0("i"), ("い".to_string(), 3));
        assert_eq!(preset_to_jp_v0("u"), ("う".to_string(), 3));
        assert_eq!(preset_to_jp_v0("e"), ("え".to_string(), 3));
        assert_eq!(preset_to_jp_v0("o"), ("お".to_string(), 3));
    }

    #[test]
    fn test_v1_lip_sync_presets() {
        assert_eq!(preset_to_jp_v1("aa"), ("あ".to_string(), 3));
        assert_eq!(preset_to_jp_v1("ih"), ("い".to_string(), 3));
        assert_eq!(preset_to_jp_v1("ou"), ("う".to_string(), 3));
        assert_eq!(preset_to_jp_v1("ee"), ("え".to_string(), 3));
        assert_eq!(preset_to_jp_v1("oh"), ("お".to_string(), 3));
    }

    #[test]
    fn test_v0_blink_presets() {
        assert_eq!(preset_to_jp_v0("blink"), ("まばたき".to_string(), 2));
        assert_eq!(preset_to_jp_v0("blink_l"), ("ウィンク".to_string(), 2));
        assert_eq!(preset_to_jp_v0("blink_r"), ("ウィンク右".to_string(), 2));
    }

    #[test]
    fn test_v1_emotion_presets() {
        assert_eq!(preset_to_jp_v1("happy"), ("喜び".to_string(), 1));
        assert_eq!(preset_to_jp_v1("angry"), ("怒り".to_string(), 1));
        assert_eq!(preset_to_jp_v1("sad"), ("悲しむ".to_string(), 1));
        assert_eq!(preset_to_jp_v1("relaxed"), ("楽しい".to_string(), 1));
        assert_eq!(preset_to_jp_v1("surprised"), ("驚き".to_string(), 1));
    }

    #[test]
    fn test_v1_gaze_presets() {
        assert_eq!(preset_to_jp_v1("lookUp"), ("上".to_string(), 4));
        assert_eq!(preset_to_jp_v1("lookDown"), ("下".to_string(), 4));
        assert_eq!(preset_to_jp_v1("lookLeft"), ("左".to_string(), 4));
        assert_eq!(preset_to_jp_v1("lookRight"), ("右".to_string(), 4));
    }

    #[test]
    fn test_unknown_preset_passthrough() {
        assert_eq!(
            preset_to_jp_v0("custom_face"),
            ("custom_face".to_string(), 4)
        );
        assert_eq!(
            preset_to_jp_v1("myExpression"),
            ("myExpression".to_string(), 4)
        );
    }

    #[test]
    fn test_v0_case_insensitive() {
        assert_eq!(preset_to_jp_v0("A"), ("あ".to_string(), 3));
        assert_eq!(preset_to_jp_v0("Blink"), ("まばたき".to_string(), 2));
        assert_eq!(preset_to_jp_v0("BLINK_L"), ("ウィンク".to_string(), 2));
    }

    #[test]
    fn test_v1_case_sensitive() {
        assert_eq!(preset_to_jp_v1("AA").0, "AA");
        assert_eq!(preset_to_jp_v1("aa").0, "あ");
    }
}
