/// VRM 0.0 プリセット名 → (PMX日本語名, パネル)
pub fn preset_to_jp_v0(preset: &str) -> (String, u8) {
    let (name, panel) = match preset.to_lowercase().as_str() {
        "a"        => ("あ", 3),
        "i"        => ("い", 3),
        "u"        => ("う", 3),
        "e"        => ("え", 3),
        "o"        => ("お", 3),
        "blink"    => ("まばたき", 2),
        "blink_l"  => ("ウィンク", 2),
        "blink_r"  => ("ウィンク右", 2),
        "joy"      => ("喜び", 1),
        "angry"    => ("怒り", 1),
        "sorrow"   => ("悲しむ", 1),
        "fun"      => ("楽しい", 1),
        "neutral"  => ("ニュートラル", 4),
        "lookup"   => ("上", 4),
        "lookdown" => ("下", 4),
        "lookleft" => ("左", 4),
        "lookright"=> ("右", 4),
        _          => return (preset.to_string(), 4),
    };
    (name.to_string(), panel)
}

/// VRM 1.0 プリセット名 → (PMX日本語名, パネル)
pub fn preset_to_jp_v1(preset: &str) -> (String, u8) {
    let (name, panel) = match preset {
        "aa"         => ("あ", 3),
        "ih"         => ("い", 3),
        "ou"         => ("う", 3),
        "ee"         => ("え", 3),
        "oh"         => ("お", 3),
        "blink"      => ("まばたき", 2),
        "blinkLeft"  => ("ウィンク", 2),
        "blinkRight" => ("ウィンク右", 2),
        "happy"      => ("喜び", 1),
        "angry"      => ("怒り", 1),
        "sad"        => ("悲しむ", 1),
        "relaxed"    => ("楽しい", 1),
        "surprised"  => ("驚き", 1),
        "neutral"    => ("ニュートラル", 4),
        "lookUp"     => ("上", 4),
        "lookDown"   => ("下", 4),
        "lookLeft"   => ("左", 4),
        "lookRight"  => ("右", 4),
        _            => return (preset.to_string(), 4),
    };
    (name.to_string(), panel)
}
