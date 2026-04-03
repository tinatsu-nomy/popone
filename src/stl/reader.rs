use crate::error::{PoponeError, Result};
use byteorder::{LittleEndian, ReadBytesExt};
use glam::Vec3;
use std::io::Cursor;

/// STL 三角形
#[derive(Debug, Clone)]
pub struct StlTriangle {
    pub normal: Vec3,
    pub vertices: [Vec3; 3],
}

/// STL モデル（パース済み）
#[derive(Debug)]
pub struct StlModel {
    pub name: String,
    pub triangles: Vec<StlTriangle>,
}

/// STL ファイルをパスから読み込む
pub fn read_stl(path: &std::path::Path) -> Result<StlModel> {
    let data = std::fs::read(path)?;
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("STL Model")
        .to_string();
    read_stl_from_data(&data, &name)
}

/// STL データをメモリから読み込む
pub fn read_stl_from_data(data: &[u8], name: &str) -> Result<StlModel> {
    if data.len() < 84 {
        // 最低限のバイナリヘッダサイズ未満 → ASCII を試行
        return parse_ascii(data, name);
    }

    // バイナリ長整合チェック: 84 + tri_count * 50 == data.len()
    let tri_count = {
        let mut cursor = Cursor::new(&data[80..84]);
        cursor.read_u32::<LittleEndian>().unwrap_or(0) as usize
    };

    let expected_len = 84usize.saturating_add(tri_count.saturating_mul(50));
    if expected_len == data.len() && tri_count > 0 {
        parse_binary(data, name, tri_count)
    } else {
        // バイナリ長が合わない → ASCII として試行
        parse_ascii(data, name)
    }
}

fn parse_binary(data: &[u8], name: &str, tri_count: usize) -> Result<StlModel> {
    let expected = 84 + tri_count * 50;
    if data.len() < expected {
        return Err(PoponeError::StlParse(format!(
            "バイナリ STL データ長不足: 期待 {} バイト、実際 {} バイト",
            expected,
            data.len()
        )));
    }

    let mut triangles = Vec::with_capacity(tri_count);
    let mut cursor = Cursor::new(&data[84..]);

    for i in 0..tri_count {
        let normal = read_vec3(&mut cursor).map_err(|e| {
            PoponeError::StlParse(format!("三角形 {} の法線読み込み失敗: {}", i, e))
        })?;
        let v0 = read_vec3(&mut cursor).map_err(|e| {
            PoponeError::StlParse(format!("三角形 {} の頂点0読み込み失敗: {}", i, e))
        })?;
        let v1 = read_vec3(&mut cursor).map_err(|e| {
            PoponeError::StlParse(format!("三角形 {} の頂点1読み込み失敗: {}", i, e))
        })?;
        let v2 = read_vec3(&mut cursor).map_err(|e| {
            PoponeError::StlParse(format!("三角形 {} の頂点2読み込み失敗: {}", i, e))
        })?;
        // 属性バイト（2バイト）をスキップ
        let _attr = cursor.read_u16::<LittleEndian>().map_err(|e| {
            PoponeError::StlParse(format!("三角形 {} の属性読み込み失敗: {}", i, e))
        })?;

        triangles.push(StlTriangle {
            normal,
            vertices: [v0, v1, v2],
        });
    }

    Ok(StlModel {
        name: name.to_string(),
        triangles,
    })
}

fn parse_ascii(data: &[u8], name: &str) -> Result<StlModel> {
    let text = std::str::from_utf8(data)
        .map_err(|e| PoponeError::StlParse(format!("ASCII STL の UTF-8 デコード失敗: {}", e)))?;

    let mut triangles = Vec::new();
    let mut lines = text.lines().map(|l| l.trim());
    let mut model_name = name.to_string();

    // "solid <name>" ヘッダ
    if let Some(first) = lines.next() {
        if let Some(n) = first.strip_prefix("solid") {
            let n = n.trim();
            if !n.is_empty() {
                model_name = n.to_string();
            }
        }
    }

    while let Some(line) = lines.next() {
        if !line.starts_with("facet normal") {
            if line.starts_with("endsolid") {
                break;
            }
            continue;
        }

        // facet normal nx ny nz
        let normal = parse_vec3_from_line(line, "facet normal")?;

        // outer loop
        let loop_line = lines
            .next()
            .ok_or_else(|| PoponeError::StlParse("outer loop が見つかりません".into()))?;
        if !loop_line.starts_with("outer loop") {
            return Err(PoponeError::StlParse(format!(
                "期待: 'outer loop'、実際: '{}'",
                loop_line
            )));
        }

        // vertex x y z × 3
        let mut verts = [Vec3::ZERO; 3];
        for v in &mut verts {
            let vline = lines
                .next()
                .ok_or_else(|| PoponeError::StlParse("vertex 行が不足しています".into()))?;
            *v = parse_vec3_from_line(vline, "vertex")?;
        }

        // endloop, endfacet
        let _ = lines.next(); // endloop
        let _ = lines.next(); // endfacet

        triangles.push(StlTriangle {
            normal,
            vertices: verts,
        });
    }

    if triangles.is_empty() {
        return Err(PoponeError::StlParse(
            "三角形が見つかりません（空の STL ファイル）".into(),
        ));
    }

    Ok(StlModel {
        name: model_name,
        triangles,
    })
}

fn parse_vec3_from_line(line: &str, prefix: &str) -> Result<Vec3> {
    let rest = line
        .strip_prefix(prefix)
        .ok_or_else(|| PoponeError::StlParse(format!("期待: '{prefix}'、実際: '{line}'")))?
        .trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() < 3 {
        return Err(PoponeError::StlParse(format!(
            "Vec3 パース失敗（要素不足）: '{}'",
            rest
        )));
    }
    let x: f32 = parts[0]
        .parse()
        .map_err(|e| PoponeError::StlParse(format!("float パース失敗: {}", e)))?;
    let y: f32 = parts[1]
        .parse()
        .map_err(|e| PoponeError::StlParse(format!("float パース失敗: {}", e)))?;
    let z: f32 = parts[2]
        .parse()
        .map_err(|e| PoponeError::StlParse(format!("float パース失敗: {}", e)))?;
    Ok(Vec3::new(x, y, z))
}

fn read_vec3(cursor: &mut Cursor<&[u8]>) -> std::io::Result<Vec3> {
    let x = cursor.read_f32::<LittleEndian>()?;
    let y = cursor.read_f32::<LittleEndian>()?;
    let z = cursor.read_f32::<LittleEndian>()?;
    Ok(Vec3::new(x, y, z))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_binary_stl_header() {
        // 最小バイナリ STL: ヘッダ80バイト + 三角形数0
        let mut data = vec![0u8; 84];
        // tri_count = 0
        data[80..84].copy_from_slice(&0u32.to_le_bytes());
        // tri_count==0 かつ長さ一致だが、空なので ASCII フォールバック → エラー
        let result = read_stl_from_data(&data, "test");
        assert!(result.is_err());
    }

    #[test]
    fn parse_binary_stl_one_triangle() {
        let mut data = vec![0u8; 84 + 50];
        // tri_count = 1
        data[80..84].copy_from_slice(&1u32.to_le_bytes());
        // 法線 (0, 0, 1)
        data[84..88].copy_from_slice(&0f32.to_le_bytes());
        data[88..92].copy_from_slice(&0f32.to_le_bytes());
        data[92..96].copy_from_slice(&1f32.to_le_bytes());
        // 頂点0 (0, 0, 0)
        // 頂点1 (1, 0, 0)
        data[108..112].copy_from_slice(&1f32.to_le_bytes());
        // 頂点2 (0, 1, 0)
        data[124..128].copy_from_slice(&1f32.to_le_bytes());

        let model = read_stl_from_data(&data, "test").unwrap();
        assert_eq!(model.triangles.len(), 1);
        assert_eq!(model.triangles[0].normal, Vec3::new(0.0, 0.0, 1.0));
    }

    #[test]
    fn parse_ascii_stl() {
        let ascii = r#"solid test
facet normal 0 0 1
  outer loop
    vertex 0 0 0
    vertex 1 0 0
    vertex 0 1 0
  endloop
endfacet
endsolid test"#;

        let model = read_stl_from_data(ascii.as_bytes(), "fallback").unwrap();
        assert_eq!(model.name, "test");
        assert_eq!(model.triangles.len(), 1);
        assert_eq!(model.triangles[0].vertices[1], Vec3::new(1.0, 0.0, 0.0));
    }
}
