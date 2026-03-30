use super::bone;
use super::parser::{FbxDocument, FbxNode};
use glam::Mat4;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionType {
    OO,
    OP,
}

#[derive(Debug, Clone)]
pub struct Connection {
    pub conn_type: ConnectionType,
    pub child_id: i64,
    pub parent_id: i64,
    pub property: Option<String>,
}

pub struct FbxObject<'a> {
    pub id: i64,
    pub name: String,
    pub sub_type: String,
    pub class: String,
    pub node: &'a FbxNode,
}

pub struct GeometryInstance<'a> {
    pub model: &'a FbxObject<'a>,
    pub geometry: &'a FbxObject<'a>,
    pub world_transform: Mat4,
    pub material_slots: Vec<MaterialSlot<'a>>,
}

pub struct MaterialSlot<'a> {
    pub slot_index: u16,
    pub material: &'a FbxObject<'a>,
}

pub struct FbxScene<'a> {
    pub objects: HashMap<i64, FbxObject<'a>>,
    pub connections: Vec<Connection>,
    children_map: HashMap<i64, Vec<i64>>,
    parents_map: HashMap<i64, Vec<i64>>,
}

impl<'a> FbxScene<'a> {
    pub fn from_document(doc: &'a FbxDocument) -> Self {
        let mut objects = HashMap::new();
        let mut connections = Vec::new();
        let mut children_map: HashMap<i64, Vec<i64>> = HashMap::new();
        let mut parents_map: HashMap<i64, Vec<i64>> = HashMap::new();

        // Parse Objects
        if let Some(objects_node) = doc.nodes.iter().find(|n| n.name == "Objects") {
            for child in &objects_node.children {
                let id = child
                    .properties
                    .first()
                    .and_then(|p| p.as_i64_value())
                    .unwrap_or(0);

                let raw_name = child
                    .properties
                    .get(1)
                    .and_then(|p| p.as_string())
                    .unwrap_or("")
                    .to_string();

                let name = if let Some(pos) = raw_name.find('\x00') {
                    raw_name[..pos].to_string()
                } else {
                    raw_name
                };

                // FBX properties[2] contains the actual sub-type (e.g. "Mesh", "LimbNode")
                let sub_type = child
                    .properties
                    .get(2)
                    .and_then(|p| p.as_string())
                    .unwrap_or("")
                    .to_string();

                let class = child.name.clone();

                objects.insert(
                    id,
                    FbxObject {
                        id,
                        name,
                        sub_type,
                        class,
                        node: child,
                    },
                );
            }
        }

        // Parse Connections
        if let Some(conn_node) = doc.nodes.iter().find(|n| n.name == "Connections") {
            for child in &conn_node.children {
                if child.name != "C" {
                    continue;
                }

                let conn_type_str = child
                    .properties
                    .first()
                    .and_then(|p| p.as_string())
                    .unwrap_or("");

                let conn_type = match conn_type_str {
                    "OO" => ConnectionType::OO,
                    "OP" => ConnectionType::OP,
                    _ => continue,
                };

                let child_id = child
                    .properties
                    .get(1)
                    .and_then(|p| p.as_i64_value())
                    .unwrap_or(0);

                let parent_id = child
                    .properties
                    .get(2)
                    .and_then(|p| p.as_i64_value())
                    .unwrap_or(0);

                let property = child
                    .properties
                    .get(3)
                    .and_then(|p| p.as_string())
                    .map(|s| s.to_string());

                children_map.entry(parent_id).or_default().push(child_id);
                parents_map.entry(child_id).or_default().push(parent_id);

                connections.push(Connection {
                    conn_type,
                    child_id,
                    parent_id,
                    property,
                });
            }
        }

        Self {
            objects,
            connections,
            children_map,
            parents_map,
        }
    }

    pub fn children_of(&self, id: i64) -> &[i64] {
        self.children_map
            .get(&id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn parents_of(&self, id: i64) -> &[i64] {
        self.parents_map
            .get(&id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get materials connected to a geometry (via parent Model)
    pub fn materials_for_geometry(&self, geom_id: i64) -> Vec<&FbxObject<'a>> {
        let model_ids = self.parents_of(geom_id);
        let mut materials = Vec::new();
        for &model_id in model_ids {
            for &child_id in self.children_of(model_id) {
                if let Some(obj) = self.objects.get(&child_id) {
                    if obj.class == "Material" {
                        materials.push(obj);
                    }
                }
            }
        }
        materials
    }

    /// Get textures connected to a material (with OP property name)
    pub fn textures_for_material(&self, mat_id: i64) -> Vec<(&FbxObject<'a>, Option<String>)> {
        self.children_of(mat_id)
            .iter()
            .filter_map(|&child_id| {
                let obj = self.objects.get(&child_id)?;
                if obj.class == "Texture" {
                    let prop = self
                        .connections
                        .iter()
                        .find(|c| c.child_id == child_id && c.parent_id == mat_id)
                        .and_then(|c| c.property.clone());
                    Some((obj, prop))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get video (embedded image) connected to a texture
    pub fn video_for_texture(&self, tex_id: i64) -> Option<&FbxObject<'a>> {
        self.children_of(tex_id)
            .iter()
            .filter_map(|id| self.objects.get(id))
            .find(|obj| obj.class == "Video")
    }

    /// Find all Geometry(Mesh) objects, sorted by ID
    pub fn geometries(&self) -> Vec<&FbxObject<'a>> {
        let mut result: Vec<_> = self
            .objects
            .values()
            .filter(|obj| {
                obj.class == "Geometry" && (obj.sub_type == "Mesh" || obj.sub_type.is_empty())
            })
            .collect();
        result.sort_by_key(|obj| obj.id);
        result
    }

    /// Geometry ごとに (先頭親 Model, Geometry, world_transform, material_slots) を返す
    pub fn geometry_instances(&self) -> Vec<GeometryInstance<'_>> {
        let geometries = self.geometries();
        let mut instances = Vec::with_capacity(geometries.len());

        for geom in &geometries {
            // 親 Model を Connection から取得（class == "Model" でフィルタ）
            let parent_models: Vec<i64> = self
                .parents_of(geom.id)
                .iter()
                .copied()
                .filter(|&pid| self.objects.get(&pid).is_some_and(|o| o.class == "Model"))
                .collect();

            let model_id = match parent_models.len() {
                0 => {
                    log::warn!(
                        "Geometry '{}' (id={}) に親 Model がありません。スキップします",
                        geom.name,
                        geom.id
                    );
                    continue;
                }
                1 => parent_models[0],
                _ => {
                    log::warn!(
                        "Geometry '{}' (id={}) に複数の親 Model があります（{}個）。先頭を使用します",
                        geom.name,
                        geom.id,
                        parent_models.len()
                    );
                    parent_models[0]
                }
            };

            let model = self.objects.get(&model_id).unwrap();
            let world_transform = self.compute_world_transform(model_id);
            let material_slots = self.material_slots_for_instance(model_id, geom.id);

            instances.push(GeometryInstance {
                model,
                geometry: geom,
                world_transform,
                material_slots,
            });
        }

        instances
    }

    /// Model に直接接続された Material を Connection 順で返す
    pub fn material_slots_for_instance(
        &self,
        model_id: i64,
        _geom_id: i64,
    ) -> Vec<MaterialSlot<'_>> {
        let mut slots = Vec::new();
        let mut slot_index: u16 = 0;

        // connections をイテレートし、parent_id == model_id かつ子の class == "Material"
        for conn in &self.connections {
            if conn.parent_id != model_id {
                continue;
            }
            if let Some(obj) = self.objects.get(&conn.child_id) {
                if obj.class == "Material" {
                    slots.push(MaterialSlot {
                        slot_index,
                        material: obj,
                    });
                    slot_index += 1;
                }
            }
        }

        slots
    }

    /// Model の階層パスを構築（"/" 区切り）
    /// 同名 sibling は ordinal 付加: 例 "Body" と "Body[1]"
    pub fn model_hierarchy_path(&self, model_id: i64) -> String {
        let mut path = String::new();
        self.build_hierarchy_path(model_id, &mut path);
        path
    }

    /// 再帰的に階層パスを構築（アロケーション最適化: &mut String 引き回し）
    fn build_hierarchy_path(&self, model_id: i64, out: &mut String) {
        let obj = match self.objects.get(&model_id) {
            Some(o) if o.class == "Model" => o,
            _ => return,
        };

        // 親 Model を探す
        let parent_model_id = self
            .parents_of(model_id)
            .iter()
            .find(|&&pid| self.objects.get(&pid).is_some_and(|o| o.class == "Model"))
            .copied();

        // 親があれば先に再帰
        if let Some(pid) = parent_model_id {
            self.build_hierarchy_path(pid, out);
            out.push('/');
        }

        // 同名 sibling の ordinal を計算
        let ordinal = if let Some(pid) = parent_model_id {
            self.compute_sibling_ordinal(pid, model_id, &obj.name)
        } else {
            // ルートレベルの sibling
            self.compute_root_sibling_ordinal(model_id, &obj.name)
        };

        out.push_str(&obj.name);
        if ordinal > 0 {
            out.push('[');
            out.push_str(&ordinal.to_string());
            out.push(']');
        }
    }

    /// 同じ親の子で同名のノードの ordinal を計算
    fn compute_sibling_ordinal(&self, parent_id: i64, target_id: i64, name: &str) -> usize {
        let mut ordinal = 0;
        for &child_id in self.children_of(parent_id) {
            if let Some(obj) = self.objects.get(&child_id) {
                if obj.class == "Model" && obj.name == name {
                    if child_id == target_id {
                        return ordinal;
                    }
                    ordinal += 1;
                }
            }
        }
        0
    }

    /// ルートレベルの同名ノードの ordinal を計算
    fn compute_root_sibling_ordinal(&self, target_id: i64, name: &str) -> usize {
        // ルートレベル = 親 Model がないすべての Model
        let mut ordinal = 0;
        let mut root_models: Vec<_> = self
            .objects
            .values()
            .filter(|o| {
                o.class == "Model"
                    && o.name == name
                    && !self
                        .parents_of(o.id)
                        .iter()
                        .any(|&pid| self.objects.get(&pid).is_some_and(|p| p.class == "Model"))
            })
            .collect();
        root_models.sort_by_key(|o| o.id);

        for m in root_models {
            if m.id == target_id {
                return ordinal;
            }
            ordinal += 1;
        }
        0
    }

    /// Model ノードのワールド変換行列を計算
    /// Model 階層を root まで辿り、各ノードのローカル変換を累積
    fn compute_world_transform(&self, model_id: i64) -> Mat4 {
        let mut chain = Vec::new();
        let mut current_id = model_id;
        loop {
            if let Some(obj) = self.objects.get(&current_id) {
                if obj.class == "Model" {
                    let local = Self::compute_model_local_transform(obj.node);
                    chain.push(local);
                }
            }
            let parent = self
                .parents_of(current_id)
                .iter()
                .find(|&&pid| self.objects.get(&pid).is_some_and(|o| o.class == "Model"))
                .copied();
            match parent {
                Some(pid) => current_id = pid,
                None => break,
            }
        }

        // root → leaf の順で累積
        let mut world = Mat4::IDENTITY;
        for local in chain.into_iter().rev() {
            world *= local;
        }
        world
    }

    /// Model ノードのローカル変換行列（T * PreRotation * Rotation * S）
    fn compute_model_local_transform(node: &FbxNode) -> Mat4 {
        let (translation, rotation_euler, pre_rotation_euler, scale) =
            bone::extract_transform(node);
        let pre_rot = bone::euler_deg_to_quat(pre_rotation_euler);
        let rot = bone::euler_deg_to_quat(rotation_euler);
        let combined = pre_rot * rot;
        Mat4::from_scale_rotation_translation(scale, combined, translation)
    }
}
