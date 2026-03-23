#[derive(Debug, Clone, PartialEq)]
pub enum VrmVersion {
    V0,
    V1,
    Unknown,
}

pub fn detect_version(document: &gltf::Document) -> VrmVersion {
    let json = document.as_json();

    if let Some(exts) = &json.extensions {
        if exts.others.contains_key("VRMC_vrm") {
            return VrmVersion::V1;
        }
        if exts.others.contains_key("VRM") {
            return VrmVersion::V0;
        }
    }

    VrmVersion::Unknown
}
