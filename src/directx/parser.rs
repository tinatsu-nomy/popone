use crate::error::{PoponeError, Result};
use glam::{Mat4, Vec2, Vec3};
use rust_i18n::t;
use std::collections::HashMap;

/// Parsed DirectX `.x` model.
#[derive(Debug)]
pub struct XModel {
    pub name: String,
    pub frames: Vec<XFrame>,
    pub meshes: Vec<XMesh>,
}

/// Frame (hierarchy node).
#[derive(Debug)]
pub struct XFrame {
    pub name: String,
    pub transform: Mat4,
    pub parent: Option<usize>,
}

/// Mesh data.
#[derive(Debug)]
pub struct XMesh {
    pub name: String,
    pub positions: Vec<Vec3>,
    /// Triangulated indices (groups of three).
    pub indices: Vec<u32>,
    pub normals: Option<XMeshNormals>,
    pub texcoords: Option<Vec<Vec2>>,
    pub materials: Option<XMeshMaterialList>,
    pub frame_index: Option<usize>,
    /// Whether SkinWeights data was detected.
    pub has_skin_weights: bool,
}

/// MeshNormals
#[derive(Debug)]
pub struct XMeshNormals {
    pub normals: Vec<Vec3>,
    /// Per-face normal indices (triangulated, in groups of three).
    pub face_normals: Vec<u32>,
}

/// MeshMaterialList
#[derive(Debug)]
pub struct XMeshMaterialList {
    /// Triangulated: per-triangle material index.
    pub face_material_indices: Vec<usize>,
    pub materials: Vec<XMaterial>,
    /// Unresolved forward references: (material slot index, referenced name).
    pub unresolved_refs: Vec<(usize, String)>,
}

/// Material
#[derive(Debug, Clone)]
pub struct XMaterial {
    pub name: String,
    pub diffuse: [f32; 4],
    pub specular_power: f32,
    pub specular: [f32; 3],
    pub emissive: [f32; 3],
    pub texture_filename: Option<String>,
}

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

/// Minimal token set.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Str(String),
    Num(String),
    LBrace,
    RBrace,
    Semi,
    Comma,
}

fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            // Whitespace
            c if c.is_whitespace() => {
                chars.next();
            }
            // Comments: `//` to end-of-line, `#` to end-of-line
            '/' if chars.clone().nth(1) == Some('/') => {
                while let Some(&c) = chars.peek() {
                    chars.next();
                    if c == '\n' {
                        break;
                    }
                }
            }
            '#' => {
                while let Some(&c) = chars.peek() {
                    chars.next();
                    if c == '\n' {
                        break;
                    }
                }
            }
            '{' => {
                tokens.push(Token::LBrace);
                chars.next();
            }
            '}' => {
                tokens.push(Token::RBrace);
                chars.next();
            }
            ';' => {
                tokens.push(Token::Semi);
                chars.next();
            }
            ',' => {
                tokens.push(Token::Comma);
                chars.next();
            }
            '"' => {
                chars.next(); // skip opening quote
                let mut s = String::new();
                while let Some(&c) = chars.peek() {
                    chars.next();
                    if c == '"' {
                        break;
                    }
                    s.push(c);
                }
                tokens.push(Token::Str(s));
            }
            // Numbers (including a leading '-')
            c if c == '-' || c == '+' || c.is_ascii_digit() || c == '.' => {
                let mut num = String::new();
                // Sign
                if ch == '-' || ch == '+' {
                    num.push(ch);
                    chars.next();
                }
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_digit()
                        || c == '.'
                        || c == 'e'
                        || c == 'E'
                        || c == '-'
                        || c == '+'
                    {
                        // Includes exponent notation like 'e-'. A non-leading '-' is only allowed
                        // immediately after the exponent letter.
                        if (c == '-' || c == '+') && !num.ends_with('e') && !num.ends_with('E') {
                            break;
                        }
                        num.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Num(num));
            }
            // Identifiers (alphanumerics + '_' + '-')
            c if c.is_alphanumeric() || c == '_' => {
                let mut id = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' || c == '-' {
                        id.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Ident(id));
            }
            // Skip UUIDs of the form <...>
            '<' => {
                while let Some(&c) = chars.peek() {
                    chars.next();
                    if c == '>' {
                        break;
                    }
                }
            }
            _ => {
                chars.next(); // Skip unknown characters
            }
        }
    }

    tokens
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    frames: Vec<XFrame>,
    meshes: Vec<XMesh>,
    /// Top-level Material name -> definition lookup.
    global_materials: HashMap<String, XMaterial>,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            frames: Vec::new(),
            meshes: Vec::new(),
            global_materials: HashMap::new(),
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.pos);
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn expect_lbrace(&mut self) -> Result<()> {
        match self.next() {
            Some(Token::LBrace) => Ok(()),
            other => Err(PoponeError::DirectXParse(
                t!(
                    "error.directx.expected_brace",
                    actual = format!("{:?}", other)
                )
                .to_string(),
            )),
        }
    }

    fn expect_semi(&mut self) {
        if matches!(self.peek(), Some(Token::Semi)) {
            self.pos += 1;
        }
    }

    fn expect_semi_or_comma(&mut self) {
        match self.peek() {
            Some(Token::Semi) | Some(Token::Comma) => {
                self.pos += 1;
            }
            _ => {}
        }
    }

    fn read_float(&mut self) -> Result<f32> {
        match self.next() {
            Some(Token::Num(s)) => s.parse::<f32>().map_err(|e| {
                PoponeError::DirectXParse(
                    t!("error.directx.float_parse_failed", detail = e.to_string()).to_string(),
                )
            }),
            other => Err(PoponeError::DirectXParse(
                t!(
                    "error.directx.expected_number",
                    actual = format!("{:?}", other)
                )
                .to_string(),
            )),
        }
    }

    fn read_int(&mut self) -> Result<u32> {
        let int_err = |e: std::num::ParseIntError| {
            PoponeError::DirectXParse(
                t!("error.directx.int_parse_failed", detail = e.to_string()).to_string(),
            )
        };
        match self.next() {
            Some(Token::Num(s)) => {
                // Truncate at a decimal point if present
                if let Some(dot) = s.find('.') {
                    s[..dot].parse::<u32>().map_err(int_err)
                } else {
                    s.parse::<u32>().map_err(int_err)
                }
            }
            other => Err(PoponeError::DirectXParse(
                t!(
                    "error.directx.expected_integer",
                    actual = format!("{:?}", other)
                )
                .to_string(),
            )),
        }
    }

    fn read_string(&mut self) -> Result<String> {
        match self.next() {
            Some(Token::Str(s)) => Ok(s.clone()),
            other => Err(PoponeError::DirectXParse(
                t!(
                    "error.directx.expected_string",
                    actual = format!("{:?}", other)
                )
                .to_string(),
            )),
        }
    }

    /// Read a name (concatenating Ident/Num tokens until the next LBrace).
    /// "Cube.001" arrives as Ident("Cube") + Num(".001"), and we recombine it into "Cube.001".
    fn read_optional_name(&mut self) -> String {
        let mut name = String::new();
        while let Some(Token::Ident(_)) | Some(Token::Num(_)) = self.peek() {
            match self.next() {
                Some(Token::Ident(s)) => name.push_str(s),
                Some(Token::Num(s)) => name.push_str(s),
                _ => break,
            }
        }
        name
    }

    /// Skip a block by tracking the `{` / `}` balance.
    fn skip_block(&mut self) {
        let mut depth = 1;
        while depth > 0 {
            match self.next() {
                Some(Token::LBrace) => depth += 1,
                Some(Token::RBrace) => depth -= 1,
                None => break,
                _ => {}
            }
        }
    }

    /// Parse the top level.
    fn parse(&mut self) -> Result<()> {
        while self.pos < self.tokens.len() {
            match self.peek() {
                Some(Token::Ident(id)) => {
                    let id = id.clone();
                    match id.as_str() {
                        "template" => {
                            self.pos += 1;
                            // Skip the template name
                            self.read_optional_name();
                            if self.peek() == Some(&Token::LBrace) {
                                self.pos += 1;
                                self.skip_block();
                            }
                        }
                        "xof" => {
                            // Header line: xof 0303txt 0032
                            // "0303txt" is split into Num("0303") + Ident("txt"), so up to 4 tokens.
                            self.pos += 1;
                            for _ in 0..4 {
                                match self.peek() {
                                    Some(Token::Num(_)) => self.pos += 1,
                                    Some(Token::Ident(id))
                                        if !matches!(
                                            id.as_str(),
                                            "Frame" | "Mesh" | "template" | "Material"
                                        ) =>
                                    {
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                        }
                        "Frame" => {
                            self.pos += 1;
                            self.parse_frame(None)?;
                        }
                        "Mesh" => {
                            self.pos += 1;
                            self.parse_mesh(None)?;
                        }
                        "Material" => {
                            // Register top-level Materials in the global table
                            self.pos += 1;
                            let mat = self.parse_material()?;
                            if !mat.name.is_empty() {
                                self.global_materials.insert(mat.name.clone(), mat);
                            }
                        }
                        _ => {
                            // Unknown top-level template -> skip the block
                            self.pos += 1;
                            self.read_optional_name();
                            if self.peek() == Some(&Token::LBrace) {
                                self.pos += 1;
                                self.skip_block();
                            }
                        }
                    }
                }
                Some(Token::RBrace) => {
                    // Skip stray closing braces
                    self.pos += 1;
                }
                _ => {
                    self.pos += 1;
                }
            }
        }

        // Second pass: rebind unresolved forward-reference materials after parsing
        for mesh in &mut self.meshes {
            if let Some(mat_list) = &mut mesh.materials {
                for (slot_idx, ref_name) in &mat_list.unresolved_refs {
                    if let Some(resolved) = self.global_materials.get(ref_name) {
                        if *slot_idx < mat_list.materials.len() {
                            mat_list.materials[*slot_idx] = resolved.clone();
                            log::info!("Forward-reference material '{}' resolved", ref_name);
                        }
                    } else {
                        log::warn!(
                            "Material '{}' not found even after parsing (slot {})",
                            ref_name,
                            slot_idx
                        );
                    }
                }
                mat_list.unresolved_refs.clear();
            }
        }

        Ok(())
    }

    /// Parse a Frame block.
    fn parse_frame(&mut self, parent: Option<usize>) -> Result<()> {
        let name = self.read_optional_name();
        self.expect_lbrace()?;

        let frame_idx = self.frames.len();
        self.frames.push(XFrame {
            name: name.clone(),
            transform: Mat4::IDENTITY,
            parent,
        });

        // Frame body
        while self.pos < self.tokens.len() {
            match self.peek() {
                Some(Token::RBrace) => {
                    self.pos += 1;
                    break;
                }
                Some(Token::Ident(id)) => {
                    let id = id.clone();
                    match id.as_str() {
                        "FrameTransformMatrix" => {
                            self.pos += 1;
                            let mat = self.parse_frame_transform_matrix()?;
                            self.frames[frame_idx].transform = mat;
                        }
                        "Frame" => {
                            self.pos += 1;
                            self.parse_frame(Some(frame_idx))?;
                        }
                        "Mesh" => {
                            self.pos += 1;
                            self.parse_mesh(Some(frame_idx))?;
                        }
                        "Material" => {
                            self.pos += 1;
                            let mat = self.parse_material()?;
                            if !mat.name.is_empty() {
                                self.global_materials.insert(mat.name.clone(), mat);
                            }
                        }
                        _ => {
                            // Skip unknown templates
                            self.pos += 1;
                            self.read_optional_name();
                            if self.peek() == Some(&Token::LBrace) {
                                self.pos += 1;
                                self.skip_block();
                            }
                        }
                    }
                }
                _ => {
                    self.pos += 1;
                }
            }
        }
        Ok(())
    }

    /// Parse FrameTransformMatrix (4x4 matrix).
    fn parse_frame_transform_matrix(&mut self) -> Result<Mat4> {
        self.read_optional_name();
        self.expect_lbrace()?;

        let mut m = [0.0f32; 16];
        for v in &mut m {
            *v = self.read_float()?;
            self.expect_semi_or_comma();
        }
        self.expect_semi();

        // .x stores matrices row-major; glam is column-major -> transpose
        let mat = Mat4::from_cols_array(&m).transpose();

        // Closing brace
        if matches!(self.peek(), Some(Token::RBrace)) {
            self.pos += 1;
        }
        Ok(mat)
    }

    /// Parse a Mesh block.
    fn parse_mesh(&mut self, frame_index: Option<usize>) -> Result<()> {
        let name = self.read_optional_name();
        self.expect_lbrace()?;
        let mut skin_warned = false;

        // Vertex count
        let vert_count = self.read_int()? as usize;
        self.expect_semi();

        // Vertex positions
        let mut positions = Vec::with_capacity(vert_count);
        for _ in 0..vert_count {
            let x = self.read_float()?;
            self.expect_semi_or_comma();
            let y = self.read_float()?;
            self.expect_semi_or_comma();
            let z = self.read_float()?;
            self.expect_semi();
            self.expect_semi_or_comma();
            positions.push(Vec3::new(x, y, z));
        }

        // Face count
        let face_count = self.read_int()? as usize;
        self.expect_semi();

        // Face indices (triangulated) plus the original per-face index counts
        let mut indices = Vec::new();
        let mut face_tri_counts: Vec<usize> = Vec::with_capacity(face_count);
        for _ in 0..face_count {
            let n = self.read_int()? as usize;
            self.expect_semi_or_comma();
            let mut face_indices = Vec::with_capacity(n);
            for j in 0..n {
                let idx = self.read_int()?;
                face_indices.push(idx);
                if j + 1 < n {
                    self.expect_semi_or_comma();
                }
            }
            self.expect_semi();
            self.expect_semi_or_comma();

            // Triangulate via fan splitting
            let tri_count = if n >= 3 { n - 2 } else { 0 };
            for t in 0..tri_count {
                indices.push(face_indices[0]);
                indices.push(face_indices[t + 1]);
                indices.push(face_indices[t + 2]);
            }
            face_tri_counts.push(tri_count);
        }

        let mut mesh = XMesh {
            name,
            positions,
            indices,
            normals: None,
            texcoords: None,
            materials: None,
            frame_index,
            has_skin_weights: false,
        };

        // Sub-templates inside the Mesh
        while self.pos < self.tokens.len() {
            match self.peek() {
                Some(Token::RBrace) => {
                    self.pos += 1;
                    break;
                }
                Some(Token::Ident(id)) => {
                    let id = id.clone();
                    match id.as_str() {
                        "MeshNormals" => {
                            self.pos += 1;
                            mesh.normals = Some(self.parse_mesh_normals(&face_tri_counts)?);
                        }
                        "MeshTextureCoords" => {
                            self.pos += 1;
                            mesh.texcoords = Some(self.parse_mesh_texcoords()?);
                        }
                        "MeshMaterialList" => {
                            self.pos += 1;
                            mesh.materials = Some(self.parse_mesh_material_list(&face_tri_counts)?);
                        }
                        "SkinWeights" => {
                            mesh.has_skin_weights = true;
                            if !skin_warned {
                                log::warn!(
                                    "SkinWeights detected. Skinned .x files are not supported"
                                );
                                skin_warned = true;
                            }
                            self.pos += 1;
                            self.read_optional_name();
                            if self.peek() == Some(&Token::LBrace) {
                                self.pos += 1;
                                self.skip_block();
                            }
                        }
                        "XSkinMeshHeader" => {
                            log::debug!("XSkinMeshHeader detected (metadata only, skipped)");
                            self.pos += 1;
                            self.read_optional_name();
                            if self.peek() == Some(&Token::LBrace) {
                                self.pos += 1;
                                self.skip_block();
                            }
                        }
                        _ => {
                            // Skip unknown sub-templates (e.g. MeshVertexColors)
                            self.pos += 1;
                            self.read_optional_name();
                            if self.peek() == Some(&Token::LBrace) {
                                self.pos += 1;
                                self.skip_block();
                            }
                        }
                    }
                }
                _ => {
                    self.pos += 1;
                }
            }
        }

        self.meshes.push(mesh);
        Ok(())
    }

    /// Parse MeshNormals.
    fn parse_mesh_normals(&mut self, face_tri_counts: &[usize]) -> Result<XMeshNormals> {
        self.read_optional_name();
        self.expect_lbrace()?;

        let normal_count = self.read_int()? as usize;
        self.expect_semi();

        let mut normals = Vec::with_capacity(normal_count);
        for _ in 0..normal_count {
            let x = self.read_float()?;
            self.expect_semi_or_comma();
            let y = self.read_float()?;
            self.expect_semi_or_comma();
            let z = self.read_float()?;
            self.expect_semi();
            self.expect_semi_or_comma();
            normals.push(Vec3::new(x, y, z));
        }

        // Face-normal indices
        let face_count = self.read_int()? as usize;
        self.expect_semi();

        let mut face_normals = Vec::new();
        for (fi, _) in (0..face_count).enumerate() {
            let n = self.read_int()? as usize;
            self.expect_semi_or_comma();
            let mut ni = Vec::with_capacity(n);
            for j in 0..n {
                let idx = self.read_int()?;
                ni.push(idx);
                if j + 1 < n {
                    self.expect_semi_or_comma();
                }
            }
            self.expect_semi();
            self.expect_semi_or_comma();

            // Triangulate (same fan-split as the face indices)
            let tri_count =
                face_tri_counts
                    .get(fi)
                    .copied()
                    .unwrap_or(if n >= 3 { n - 2 } else { 0 });
            // Sanity-check the normal-index count against the triangle count
            let safe_tri_count = if tri_count > 0 && n >= 3 {
                tri_count.min(n - 2)
            } else {
                0
            };
            for t in 0..safe_tri_count {
                face_normals.push(ni[0]);
                face_normals.push(ni[t + 1]);
                face_normals.push(ni[t + 2]);
            }
        }

        // Closing brace
        if matches!(self.peek(), Some(Token::RBrace)) {
            self.pos += 1;
        }

        Ok(XMeshNormals {
            normals,
            face_normals,
        })
    }

    /// Parse MeshTextureCoords.
    fn parse_mesh_texcoords(&mut self) -> Result<Vec<Vec2>> {
        self.read_optional_name();
        self.expect_lbrace()?;

        let count = self.read_int()? as usize;
        self.expect_semi();

        let mut coords = Vec::with_capacity(count);
        for _ in 0..count {
            let u = self.read_float()?;
            self.expect_semi_or_comma();
            let v = self.read_float()?;
            self.expect_semi();
            self.expect_semi_or_comma();
            coords.push(Vec2::new(u, v));
        }

        if matches!(self.peek(), Some(Token::RBrace)) {
            self.pos += 1;
        }

        Ok(coords)
    }

    /// Parse MeshMaterialList.
    fn parse_mesh_material_list(&mut self, face_tri_counts: &[usize]) -> Result<XMeshMaterialList> {
        self.read_optional_name();
        self.expect_lbrace()?;

        let mat_count = self.read_int()? as usize;
        self.expect_semi();

        let face_count = self.read_int()? as usize;
        self.expect_semi();

        // Per-face material indices (pre-triangulation)
        let mut orig_face_mat = Vec::with_capacity(face_count);
        for i in 0..face_count {
            let idx = self.read_int()? as usize;
            orig_face_mat.push(idx);
            if i + 1 < face_count {
                self.expect_semi_or_comma();
            }
        }
        self.expect_semi();
        self.expect_semi_or_comma();

        // Expand to per-triangle material indices after triangulation
        let mut face_material_indices = Vec::new();
        for (fi, &mat_idx) in orig_face_mat.iter().enumerate() {
            let tri_count = face_tri_counts.get(fi).copied().unwrap_or(1);
            for _ in 0..tri_count {
                face_material_indices.push(mat_idx);
            }
        }

        // Material blocks
        let mut materials = Vec::with_capacity(mat_count);
        let mut unresolved_refs: Vec<(usize, String)> = Vec::new();
        while self.pos < self.tokens.len() {
            match self.peek() {
                Some(Token::RBrace) => {
                    self.pos += 1;
                    break;
                }
                Some(Token::Ident(id)) if id == "Material" => {
                    self.pos += 1;
                    let mat = self.parse_material()?;
                    // Register named materials in the global table too (for cross-mesh references and the 2-pass rebind)
                    if !mat.name.is_empty() {
                        self.global_materials.insert(mat.name.clone(), mat.clone());
                    }
                    materials.push(mat);
                }
                Some(Token::Ident(id)) if id == "SI_PhongMaterial" || id == "EffectInstance" => {
                    // Skip unknown material templates
                    self.pos += 1;
                    self.read_optional_name();
                    if self.peek() == Some(&Token::LBrace) {
                        self.pos += 1;
                        self.skip_block();
                    }
                }
                Some(Token::LBrace) => {
                    // Reference block ({ MaterialName }) -- resolve from the global table
                    self.pos += 1;
                    // Support dotted names too ("Material.001" -> Ident + Num concatenated)
                    let ref_name = self.read_optional_name();
                    if !ref_name.is_empty() {
                        if let Some(mat) = self.global_materials.get(&ref_name) {
                            materials.push(mat.clone());
                        } else {
                            // Possibly a forward reference: insert a placeholder and resolve later
                            let slot_idx = materials.len();
                            unresolved_refs.push((slot_idx, ref_name.clone()));
                            log::debug!(
                                "Forward-reference material '{}' tentatively registered to slot {}",
                                ref_name,
                                slot_idx
                            );
                            materials.push(XMaterial {
                                name: format!("placeholder_{}", ref_name),
                                diffuse: [0.8, 0.8, 0.8, 1.0],
                                specular_power: 0.0,
                                specular: [0.0, 0.0, 0.0],
                                emissive: [0.0, 0.0, 0.0],
                                texture_filename: None,
                            });
                        }
                        // Skip the closing brace
                        if matches!(self.peek(), Some(Token::RBrace)) {
                            self.pos += 1;
                        }
                    } else {
                        self.skip_block();
                    }
                }
                _ => {
                    self.pos += 1;
                }
            }
        }

        // Fill with placeholder materials when the declared count was not met
        while materials.len() < mat_count {
            let idx = materials.len();
            log::warn!(
                "Material slot {} unresolved, falling back to default material",
                idx
            );
            materials.push(XMaterial {
                name: format!("placeholder_{}", idx),
                diffuse: [0.8, 0.8, 0.8, 1.0],
                specular_power: 0.0,
                specular: [0.0, 0.0, 0.0],
                emissive: [0.0, 0.0, 0.0],
                texture_filename: None,
            });
        }

        Ok(XMeshMaterialList {
            face_material_indices,
            materials,
            unresolved_refs,
        })
    }

    /// Parse a Material block.
    fn parse_material(&mut self) -> Result<XMaterial> {
        let name = self.read_optional_name();
        self.expect_lbrace()?;

        // diffuse RGBA
        let dr = self.read_float()?;
        self.expect_semi_or_comma();
        let dg = self.read_float()?;
        self.expect_semi_or_comma();
        let db = self.read_float()?;
        self.expect_semi_or_comma();
        let da = self.read_float()?;
        self.expect_semi();
        self.expect_semi();

        // specular power
        let specular_power = self.read_float()?;
        self.expect_semi();

        // specular RGB
        let sr = self.read_float()?;
        self.expect_semi_or_comma();
        let sg = self.read_float()?;
        self.expect_semi_or_comma();
        let sb = self.read_float()?;
        self.expect_semi();
        self.expect_semi();

        // emissive RGB
        let er = self.read_float()?;
        self.expect_semi_or_comma();
        let eg = self.read_float()?;
        self.expect_semi_or_comma();
        let eb = self.read_float()?;
        self.expect_semi();
        self.expect_semi();

        // TextureFilename (optional)
        let mut texture_filename = None;
        while self.pos < self.tokens.len() {
            match self.peek() {
                Some(Token::RBrace) => {
                    self.pos += 1;
                    break;
                }
                Some(Token::Ident(id)) if id == "TextureFilename" => {
                    self.pos += 1;
                    self.read_optional_name();
                    self.expect_lbrace()?;
                    let filename = self.read_string()?;
                    texture_filename = Some(filename);
                    self.expect_semi();
                    if matches!(self.peek(), Some(Token::RBrace)) {
                        self.pos += 1;
                    }
                }
                Some(Token::Ident(_)) => {
                    // Skip unknown sub-blocks
                    self.pos += 1;
                    self.read_optional_name();
                    if self.peek() == Some(&Token::LBrace) {
                        self.pos += 1;
                        self.skip_block();
                    }
                }
                _ => {
                    self.pos += 1;
                }
            }
        }

        Ok(XMaterial {
            name,
            diffuse: [dr, dg, db, da],
            specular_power,
            specular: [sr, sg, sb],
            emissive: [er, eg, eb],
            texture_filename,
        })
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Read a `.x` file from a path.
pub fn read_x(path: &std::path::Path) -> Result<XModel> {
    let data = std::fs::read(path)?;
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("DirectX Model")
        .to_string();
    read_x_from_data(&data, &name)
}

/// Read `.x` data from memory.
pub fn read_x_from_data(data: &[u8], name: &str) -> Result<XModel> {
    // Shift_JIS / UTF-8 support
    // Detect binary / compressed formats
    if data.len() >= 16 {
        let header = &data[..16];
        if header.starts_with(b"xof ") {
            // Header form: e.g. "xof 0303bin 0032"
            let header_str = std::str::from_utf8(header).unwrap_or("");
            if header_str.contains("bin") {
                return Err(PoponeError::DirectXParse(
                    t!("error.directx.binary_unsupported").to_string(),
                ));
            }
            if header_str.contains("cmp") || header_str.contains("zip") {
                return Err(PoponeError::DirectXParse(
                    t!("error.directx.compressed_unsupported").to_string(),
                ));
            }
        }
    }

    // Shift_JIS / UTF-8 support
    let text = match std::str::from_utf8(data) {
        Ok(s) => s.to_string(),
        Err(_) => {
            // Fall back to Shift_JIS
            let (decoded, _, had_errors) = encoding_rs::SHIFT_JIS.decode(data);
            if had_errors {
                return Err(PoponeError::DirectXParse(
                    t!("error.directx.text_decode_failed").to_string(),
                ));
            }
            decoded.into_owned()
        }
    };

    let tokens = tokenize(&text);
    let mut parser = Parser::new(tokens);
    parser.parse()?;

    if parser.meshes.is_empty() {
        return Err(PoponeError::DirectXParse(
            t!("error.directx.no_mesh").to_string(),
        ));
    }

    Ok(XModel {
        name: name.to_string(),
        frames: parser.frames,
        meshes: parser.meshes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_X: &str = r#"xof 0303txt 0032

Frame Root {
  FrameTransformMatrix {
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    0.0, 0.0, 0.0, 1.0;;
  }

  Mesh TestMesh {
    3;
    0.0; 0.0; 0.0;,
    1.0; 0.0; 0.0;,
    0.0; 1.0; 0.0;;

    1;
    3; 0, 1, 2;;

    MeshNormals {
      1;
      0.0; 0.0; 1.0;;

      1;
      3; 0, 0, 0;;
    }

    MeshTextureCoords {
      3;
      0.0; 0.0;,
      1.0; 0.0;,
      0.0; 1.0;;
    }

    MeshMaterialList {
      1;
      1;
      0;;

      Material TestMat {
        0.8; 0.8; 0.8; 1.0;;
        10.0;
        1.0; 1.0; 1.0;;
        0.0; 0.0; 0.0;;

        TextureFilename {
          "texture.png";
        }
      }
    }
  }
}
"#;

    #[test]
    fn parse_sample_x() {
        let model = read_x_from_data(SAMPLE_X.as_bytes(), "test").unwrap();
        assert_eq!(model.meshes.len(), 1);
        assert_eq!(model.meshes[0].positions.len(), 3);
        assert_eq!(model.meshes[0].indices.len(), 3);
        assert_eq!(model.frames.len(), 1);
        assert_eq!(model.frames[0].name, "Root");

        let normals = model.meshes[0].normals.as_ref().unwrap();
        assert_eq!(normals.normals.len(), 1);
        assert_eq!(normals.normals[0], Vec3::new(0.0, 0.0, 1.0));

        let texcoords = model.meshes[0].texcoords.as_ref().unwrap();
        assert_eq!(texcoords.len(), 3);

        let mat_list = model.meshes[0].materials.as_ref().unwrap();
        assert_eq!(mat_list.materials.len(), 1);
        assert_eq!(
            mat_list.materials[0].texture_filename.as_deref(),
            Some("texture.png")
        );
    }

    #[test]
    fn parse_quad_triangulation() {
        let x_data = r#"xof 0303txt 0032
Mesh {
  4;
  0.0; 0.0; 0.0;,
  1.0; 0.0; 0.0;,
  1.0; 1.0; 0.0;,
  0.0; 1.0; 0.0;;

  1;
  4; 0, 1, 2, 3;;
}
"#;
        let model = read_x_from_data(x_data.as_bytes(), "quad").unwrap();
        // Quad -> 2 triangles = 6 indices
        assert_eq!(model.meshes[0].indices.len(), 6);
        assert_eq!(model.meshes[0].indices, vec![0, 1, 2, 0, 2, 3]);
    }
}
