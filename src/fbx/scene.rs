use std::collections::HashMap;
use super::parser::{FbxDocument, FbxNode};

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
                obj.class == "Geometry"
                    && (obj.sub_type == "Mesh" || obj.sub_type.is_empty())
            })
            .collect();
        result.sort_by_key(|obj| obj.id);
        result
    }
}
