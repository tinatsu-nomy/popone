use anyhow::{bail, Context, Result};
use byteorder::{LittleEndian, ReadBytesExt};
use flate2::read::ZlibDecoder;
use std::io::{Cursor, Read, Seek, SeekFrom};

const MAGIC: &[u8; 23] = b"Kaydara FBX Binary  \x00\x1a\x00";

#[derive(Debug)]
pub struct FbxDocument {
    pub version: u32,
    pub nodes: Vec<FbxNode>,
}

#[derive(Debug)]
pub struct FbxNode {
    pub name: String,
    pub properties: Vec<FbxProperty>,
    pub children: Vec<FbxNode>,
}

#[derive(Debug, Clone)]
pub enum FbxProperty {
    Bool(bool),
    I16(i16),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    BoolArray(Vec<bool>),
    I32Array(Vec<i32>),
    I64Array(Vec<i64>),
    F32Array(Vec<f32>),
    F64Array(Vec<f64>),
    String(String),
    Binary(Vec<u8>),
}

impl FbxNode {
    pub fn child(&self, name: &str) -> Option<&FbxNode> {
        self.children.iter().find(|c| c.name == name)
    }
}

impl FbxProperty {
    pub fn as_f64_array(&self) -> Option<&[f64]> {
        match self {
            FbxProperty::F64Array(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_i32_array(&self) -> Option<&[i32]> {
        match self {
            FbxProperty::I32Array(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_i64_array(&self) -> Option<&[i64]> {
        match self {
            FbxProperty::I64Array(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_f32_array(&self) -> Option<&[f32]> {
        match self {
            FbxProperty::F32Array(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            FbxProperty::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i64_value(&self) -> Option<i64> {
        match self {
            FbxProperty::I64(v) => Some(*v),
            FbxProperty::I32(v) => Some(*v as i64),
            FbxProperty::I16(v) => Some(*v as i64),
            _ => None,
        }
    }

    pub fn as_f64_value(&self) -> Option<f64> {
        match self {
            FbxProperty::F64(v) => Some(*v),
            FbxProperty::F32(v) => Some(*v as f64),
            FbxProperty::I32(v) => Some(*v as f64),
            FbxProperty::I64(v) => Some(*v as f64),
            _ => None,
        }
    }

    pub fn as_binary(&self) -> Option<&[u8]> {
        match self {
            FbxProperty::Binary(v) => Some(v),
            _ => None,
        }
    }
}

pub fn parse(data: &[u8]) -> Result<FbxDocument> {
    // UTF-8 BOM をスキップ
    let data = data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(data);
    // ASCII FBX 自動検出
    if data.len() >= 5 && &data[..5] == b"; FBX" {
        return parse_ascii(data);
    }

    let mut cursor = Cursor::new(data);

    // ヘッダ検証
    let mut magic = [0u8; 23];
    cursor.read_exact(&mut magic)?;
    if &magic != MAGIC {
        bail!("Invalid FBX magic number");
    }

    let version = cursor.read_u32::<LittleEndian>()?;
    log::info!("FBX version: {}", version);

    // トップレベルノードを読み取り
    let mut nodes = Vec::new();
    loop {
        let node = parse_node(&mut cursor, version)?;
        match node {
            Some(n) => nodes.push(n),
            None => break, // 終端マーカー
        }
    }

    Ok(FbxDocument { version, nodes })
}

fn parse_node(cursor: &mut Cursor<&[u8]>, version: u32) -> Result<Option<FbxNode>> {
    let (end_offset, num_properties, _property_list_len) = if version >= 7500 {
        (
            cursor.read_u64::<LittleEndian>()?,
            cursor.read_u64::<LittleEndian>()?,
            cursor.read_u64::<LittleEndian>()?,
        )
    } else {
        (
            cursor.read_u32::<LittleEndian>()? as u64,
            cursor.read_u32::<LittleEndian>()? as u64,
            cursor.read_u32::<LittleEndian>()? as u64,
        )
    };

    // 終端マーカー判定
    if end_offset == 0 {
        return Ok(None);
    }

    let name_len = cursor.read_u8()? as usize;
    let mut name_buf = vec![0u8; name_len];
    cursor.read_exact(&mut name_buf)?;
    let name = String::from_utf8_lossy(&name_buf).to_string();

    // 属性読み取り
    let mut properties = Vec::with_capacity(num_properties as usize);
    let _prop_start = cursor.position();
    for _ in 0..num_properties {
        properties.push(parse_property(cursor)?);
    }

    // 子ノード読み取り
    let mut children = Vec::new();
    while cursor.position() < end_offset {
        match parse_node(cursor, version)? {
            Some(child) => children.push(child),
            None => break,
        }
    }

    // end_offsetまでシーク（安全策）
    cursor.seek(SeekFrom::Start(end_offset))?;

    Ok(Some(FbxNode {
        name,
        properties,
        children,
    }))
}

fn parse_property(cursor: &mut Cursor<&[u8]>) -> Result<FbxProperty> {
    let type_code = cursor.read_u8()?;
    match type_code {
        // プリミティブ型
        b'C' => {
            let v = cursor.read_u8()?;
            // 0x59='Y'=true, 0x54='T'=false, Blenderバグ対応で奇偶判定
            Ok(FbxProperty::Bool(
                v == 0x59 || v == 0x01 || (v != 0x54 && v != 0x00 && v % 2 == 1),
            ))
        }
        b'Y' => Ok(FbxProperty::I16(cursor.read_i16::<LittleEndian>()?)),
        b'I' => Ok(FbxProperty::I32(cursor.read_i32::<LittleEndian>()?)),
        b'L' => Ok(FbxProperty::I64(cursor.read_i64::<LittleEndian>()?)),
        b'F' => Ok(FbxProperty::F32(cursor.read_f32::<LittleEndian>()?)),
        b'D' => Ok(FbxProperty::F64(cursor.read_f64::<LittleEndian>()?)),

        // 配列型
        b'b' => {
            let raw = read_array_raw(cursor, 1)?;
            Ok(FbxProperty::BoolArray(
                raw.into_iter().map(|b| b != 0).collect(),
            ))
        }
        b'i' => {
            let raw = read_array_raw(cursor, 4)?;
            let values = raw
                .chunks_exact(4)
                .map(|c| {
                    i32::from_le_bytes(c.try_into().expect("chunks_exact(4) guarantees 4 bytes"))
                })
                .collect();
            Ok(FbxProperty::I32Array(values))
        }
        b'l' => {
            let raw = read_array_raw(cursor, 8)?;
            let values = raw
                .chunks_exact(8)
                .map(|c| {
                    i64::from_le_bytes(c.try_into().expect("chunks_exact(8) guarantees 8 bytes"))
                })
                .collect();
            Ok(FbxProperty::I64Array(values))
        }
        b'f' => {
            let raw = read_array_raw(cursor, 4)?;
            let values = raw
                .chunks_exact(4)
                .map(|c| {
                    f32::from_le_bytes(c.try_into().expect("chunks_exact(4) guarantees 4 bytes"))
                })
                .collect();
            Ok(FbxProperty::F32Array(values))
        }
        b'd' => {
            let raw = read_array_raw(cursor, 8)?;
            let values = raw
                .chunks_exact(8)
                .map(|c| {
                    f64::from_le_bytes(c.try_into().expect("chunks_exact(8) guarantees 8 bytes"))
                })
                .collect();
            Ok(FbxProperty::F64Array(values))
        }

        // 特殊型
        b'S' => {
            let len = cursor.read_u32::<LittleEndian>()? as usize;
            let mut buf = vec![0u8; len];
            cursor.read_exact(&mut buf)?;
            Ok(FbxProperty::String(
                String::from_utf8_lossy(&buf).to_string(),
            ))
        }
        b'R' => {
            let len = cursor.read_u32::<LittleEndian>()? as usize;
            let mut buf = vec![0u8; len];
            cursor.read_exact(&mut buf)?;
            Ok(FbxProperty::Binary(buf))
        }

        _ => bail!("Unknown property type code: 0x{:02x}", type_code),
    }
}

fn read_array_raw(cursor: &mut Cursor<&[u8]>, element_size: usize) -> Result<Vec<u8>> {
    let array_len = cursor.read_u32::<LittleEndian>()? as usize;
    let encoding = cursor.read_u32::<LittleEndian>()?;
    let compressed_len = cursor.read_u32::<LittleEndian>()? as usize;

    let mut compressed = vec![0u8; compressed_len];
    cursor.read_exact(&mut compressed)?;

    let raw = match encoding {
        0 => compressed,
        1 => {
            let expected_size = array_len * element_size;
            let mut decoder = ZlibDecoder::new(&compressed[..]);
            let mut decompressed = vec![0u8; expected_size];
            decoder
                .read_exact(&mut decompressed)
                .context("zlib decompression failed")?;
            decompressed
        }
        _ => bail!("Unknown encoding: {}", encoding),
    };

    Ok(raw)
}

// ============================================================
// ASCII FBX パーサー
// ============================================================

fn parse_ascii(data: &[u8]) -> Result<FbxDocument> {
    let text = String::from_utf8_lossy(data);
    let mut parser = AsciiParser {
        lines: text.lines().collect(),
        pos: 0,
    };

    let mut nodes = Vec::new();
    while parser.pos < parser.lines.len() {
        match parser.parse_node()? {
            Some(node) => nodes.push(node),
            None => break,
        }
    }

    // FBXHeaderExtension > FBXVersion からバージョン取得
    let version = nodes
        .iter()
        .find(|n| n.name == "FBXHeaderExtension")
        .and_then(|h| h.child("FBXVersion"))
        .and_then(|v| v.properties.first())
        .and_then(|p| p.as_i64_value())
        .unwrap_or(7400) as u32;

    log::info!("FBX version: {} (ASCII)", version);

    Ok(FbxDocument { version, nodes })
}

struct AsciiParser<'a> {
    lines: Vec<&'a str>,
    pos: usize,
}

impl<'a> AsciiParser<'a> {
    fn skip_empty_and_comments(&mut self) {
        while self.pos < self.lines.len() {
            let line = self.lines[self.pos].trim();
            if !line.is_empty() && !line.starts_with(';') {
                break;
            }
            self.pos += 1;
        }
    }

    fn parse_node(&mut self) -> Result<Option<FbxNode>> {
        self.skip_empty_and_comments();
        if self.pos >= self.lines.len() {
            return Ok(None);
        }
        let line = self.lines[self.pos].trim();
        if line == "}" {
            return Ok(None);
        }
        self.pos += 1;

        // インラインコメント除去
        let line = ascii_strip_inline_comment(line);

        // ノード名と値部分を分離（引用符外の最初の `:` で分割）
        let colon_pos = ascii_find_colon(line)
            .ok_or_else(|| anyhow::anyhow!("Expected ':' in: {}", &line[..line.len().min(80)]))?;
        let name = line[..colon_pos].trim().to_string();
        let after_colon = line[colon_pos + 1..].trim();

        // 末尾の `{` を検出（同一行、または次行に `{` のみの場合）
        let (value_part, has_block) = if let Some(stripped) = after_colon.strip_suffix('{') {
            (stripped.trim(), true)
        } else {
            // 次行が `{` のみの場合
            let next_is_brace = self.pos < self.lines.len() && self.lines[self.pos].trim() == "{";
            if next_is_brace {
                self.pos += 1;
                (after_colon, true)
            } else {
                (after_colon, false)
            }
        };

        // 配列ノード: *N
        if value_part.starts_with('*') {
            let prop = self.parse_array_data(&name)?;
            return Ok(Some(FbxNode {
                name,
                properties: vec![prop],
                children: Vec::new(),
            }));
        }

        // インラインプロパティ解析
        let mut properties = if !value_part.is_empty() {
            ascii_parse_inline_values(value_part)
        } else {
            Vec::new()
        };

        // P ノードは型ヒントに基づいて properties[4+] の型を修正
        if name == "P" {
            ascii_fixup_p_node(&mut properties);
        }

        // 子ノード解析
        let mut children = Vec::new();
        if has_block {
            // Content ノード: ASCII FBX の埋め込みデータ（base64等）を特別処理
            // 行指向パーサーでは通常の子ノードとして解析できないため、`}` まで生データとして収集し
            // FbxProperty::String として保持する（上位層がデコード方法を決定できるよう）
            if name == "Content" {
                let mut raw_lines: Vec<&str> = Vec::new();
                while self.pos < self.lines.len() {
                    let l = self.lines[self.pos].trim();
                    if l == "}" {
                        self.pos += 1;
                        break;
                    }
                    raw_lines.push(l);
                    self.pos += 1;
                }
                if !raw_lines.is_empty() {
                    let joined = raw_lines.join("");
                    properties = vec![FbxProperty::String(joined)];
                    log::debug!(
                        "ASCII FBX: Content ブロック（{}行）を文字列として保持",
                        raw_lines.len()
                    );
                }
            } else {
                loop {
                    self.skip_empty_and_comments();
                    if self.pos >= self.lines.len() {
                        break;
                    }
                    if self.lines[self.pos].trim() == "}" {
                        self.pos += 1;
                        break;
                    }
                    match self.parse_node()? {
                        Some(child) => children.push(child),
                        None => {
                            // parse_node が None を返した = `}` を検出
                            if self.pos < self.lines.len() && self.lines[self.pos].trim() == "}" {
                                self.pos += 1;
                            }
                            break;
                        }
                    }
                }
            }
        }

        Ok(Some(FbxNode {
            name,
            properties,
            children,
        }))
    }

    /// 配列データ解析: `*N {` の後の `a: v1, v2, ...` 行を `}` まで読み取る
    fn parse_array_data(&mut self, node_name: &str) -> Result<FbxProperty> {
        let mut data_str = String::new();
        while self.pos < self.lines.len() {
            let line = self.lines[self.pos].trim();
            if line == "}" {
                self.pos += 1;
                break;
            }
            self.pos += 1;
            let line = ascii_strip_inline_comment(line);
            let content = line.strip_prefix("a:").unwrap_or(line).trim();
            if content.is_empty() {
                continue;
            }
            if !data_str.is_empty() && !data_str.ends_with(',') {
                data_str.push(',');
            }
            data_str.push_str(content);
        }

        let values: Vec<&str> = data_str
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        match ascii_array_type(node_name, &values) {
            AsciiArrayType::F64 => {
                let arr: Vec<f64> = values
                    .iter()
                    .map(|s| {
                        s.parse::<f64>().with_context(|| {
                            format!("ASCII FBX: '{s}' を f64 に変換できません (node: {node_name})")
                        })
                    })
                    .collect::<Result<_>>()?;
                Ok(FbxProperty::F64Array(arr))
            }
            AsciiArrayType::I32 => {
                let arr: Vec<i32> = values
                    .iter()
                    .map(|s| {
                        s.parse::<i32>().with_context(|| {
                            format!("ASCII FBX: '{s}' を i32 に変換できません (node: {node_name})")
                        })
                    })
                    .collect::<Result<_>>()?;
                Ok(FbxProperty::I32Array(arr))
            }
            AsciiArrayType::I64 => {
                let arr: Vec<i64> = values
                    .iter()
                    .map(|s| {
                        s.parse::<i64>().with_context(|| {
                            format!("ASCII FBX: '{s}' を i64 に変換できません (node: {node_name})")
                        })
                    })
                    .collect::<Result<_>>()?;
                Ok(FbxProperty::I64Array(arr))
            }
        }
    }
}

// --- ASCII FBX ヘルパー関数 ---

enum AsciiArrayType {
    F64,
    I32,
    I64,
}

/// ノード名と配列データから要素の型を推定
/// 既知ノード名 → 明確な型。不明ノード → データ内容から推定（小数点/指数表記→F64、それ以外→I32）
fn ascii_array_type(name: &str, values: &[&str]) -> AsciiArrayType {
    match name {
        "PolygonVertexIndex" | "Indexes" | "Materials" | "NormalsIndex" | "UVIndex"
        | "EdgeIndices" => AsciiArrayType::I32,
        "KeyTime" => AsciiArrayType::I64,
        "Vertices" | "Normals" | "UV" | "Weights" | "Transform" | "TransformLink" | "Matrix"
        | "KeyValueFloat" | "FullWeights" | "Binormals" | "BinormalsW" | "Tangents"
        | "TangentsW" | "NormalsW" => AsciiArrayType::F64,
        _ => {
            // 不明ノード: データ内容から推定
            let has_float = values
                .iter()
                .any(|s| s.contains('.') || s.contains('e') || s.contains('E'));
            if has_float {
                AsciiArrayType::F64
            } else {
                AsciiArrayType::I32
            }
        }
    }
}

/// 引用符外のインラインコメント（`;`以降）を除去
fn ascii_strip_inline_comment(s: &str) -> &str {
    let mut in_quotes = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ';' if !in_quotes => return s[..i].trim_end(),
            _ => {}
        }
    }
    s
}

/// 引用符の外にある最初の `:` の位置を返す
fn ascii_find_colon(s: &str) -> Option<usize> {
    let mut in_quotes = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ':' if !in_quotes => return Some(i),
            _ => {}
        }
    }
    None
}

/// カンマ区切りのプロパティ値を引用符を尊重して分割・解析
fn ascii_parse_inline_values(s: &str) -> Vec<FbxProperty> {
    ascii_split_csv(s)
        .into_iter()
        .map(|v| ascii_parse_scalar(v.trim()))
        .collect()
}

/// 引用符を尊重してカンマで分割
fn ascii_split_csv(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                result.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start <= s.len() {
        let rest = &s[start..];
        if !rest.trim().is_empty() {
            result.push(rest);
        }
    }
    result
}

/// スカラー値をテキスト表現から FbxProperty に変換
fn ascii_parse_scalar(s: &str) -> FbxProperty {
    let s = s.trim();
    if s.is_empty() {
        return FbxProperty::String(String::new());
    }
    // 引用符付き文字列
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        return FbxProperty::String(s[1..s.len() - 1].to_string());
    }
    // 浮動小数点数（小数点または指数表記）
    if s.contains('.') || s.contains('e') || s.contains('E') {
        if let Ok(v) = s.parse::<f64>() {
            return FbxProperty::F64(v);
        }
    }
    // 整数
    if let Ok(v) = s.parse::<i64>() {
        return FbxProperty::I64(v);
    }
    // フォールバック: 文字列
    FbxProperty::String(s.to_string())
}

/// P ノードの properties[4+] を型ヒント (properties[1]) に基づいて修正
fn ascii_fixup_p_node(properties: &mut [FbxProperty]) {
    if properties.len() < 5 {
        return;
    }
    let type_hint = match &properties[1] {
        FbxProperty::String(s) => s.as_str(),
        _ => return,
    };

    let is_int = matches!(
        type_hint,
        "int" | "Integer" | "enum" | "Bool" | "bool" | "Visibility" | "Visibility Inheritance"
    );
    let is_float = type_hint == "double"
        || type_hint == "Number"
        || type_hint == "Float"
        || type_hint.starts_with("Lcl ")
        || type_hint.starts_with("Vector")
        || type_hint.starts_with("Color");

    if !is_int && !is_float {
        return;
    }

    for prop in properties[4..].iter_mut() {
        if is_int {
            let v = match prop {
                FbxProperty::I64(v) => *v as i32,
                FbxProperty::F64(v) => *v as i32,
                _ => continue,
            };
            *prop = FbxProperty::I32(v);
        } else {
            let v = match prop {
                FbxProperty::I64(v) => *v as f64,
                FbxProperty::F64(v) => *v,
                _ => continue,
            };
            *prop = FbxProperty::F64(v);
        }
    }
}
