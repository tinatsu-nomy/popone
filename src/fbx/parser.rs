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
            Ok(FbxProperty::Bool(v == 0x59 || v == 0x01 || (v != 0x54 && v != 0x00 && v % 2 == 1)))
        }
        b'Y' => Ok(FbxProperty::I16(cursor.read_i16::<LittleEndian>()?)),
        b'I' => Ok(FbxProperty::I32(cursor.read_i32::<LittleEndian>()?)),
        b'L' => Ok(FbxProperty::I64(cursor.read_i64::<LittleEndian>()?)),
        b'F' => Ok(FbxProperty::F32(cursor.read_f32::<LittleEndian>()?)),
        b'D' => Ok(FbxProperty::F64(cursor.read_f64::<LittleEndian>()?)),

        // 配列型
        b'b' => {
            let raw = read_array_raw(cursor, 1)?;
            Ok(FbxProperty::BoolArray(raw.into_iter().map(|b| b != 0).collect()))
        }
        b'i' => {
            let raw = read_array_raw(cursor, 4)?;
            let values = raw.chunks_exact(4)
                .map(|c| i32::from_le_bytes(c.try_into().expect("chunks_exact(4) guarantees 4 bytes")))
                .collect();
            Ok(FbxProperty::I32Array(values))
        }
        b'l' => {
            let raw = read_array_raw(cursor, 8)?;
            let values = raw.chunks_exact(8)
                .map(|c| i64::from_le_bytes(c.try_into().expect("chunks_exact(8) guarantees 8 bytes")))
                .collect();
            Ok(FbxProperty::I64Array(values))
        }
        b'f' => {
            let raw = read_array_raw(cursor, 4)?;
            let values = raw.chunks_exact(4)
                .map(|c| f32::from_le_bytes(c.try_into().expect("chunks_exact(4) guarantees 4 bytes")))
                .collect();
            Ok(FbxProperty::F32Array(values))
        }
        b'd' => {
            let raw = read_array_raw(cursor, 8)?;
            let values = raw.chunks_exact(8)
                .map(|c| f64::from_le_bytes(c.try_into().expect("chunks_exact(8) guarantees 8 bytes")))
                .collect();
            Ok(FbxProperty::F64Array(values))
        }

        // 特殊型
        b'S' => {
            let len = cursor.read_u32::<LittleEndian>()? as usize;
            let mut buf = vec![0u8; len];
            cursor.read_exact(&mut buf)?;
            Ok(FbxProperty::String(String::from_utf8_lossy(&buf).to_string()))
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
            decoder.read_exact(&mut decompressed)
                .context("zlib decompression failed")?;
            decompressed
        }
        _ => bail!("Unknown encoding: {}", encoding),
    };

    Ok(raw)
}
