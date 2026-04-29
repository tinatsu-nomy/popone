use crate::error::{PoponeError, Result};
use byteorder::{LittleEndian, ReadBytesExt};
use glam::Vec3;
use rust_i18n::t;
use std::io::Cursor;

/// STL triangle.
#[derive(Debug, Clone)]
pub struct StlTriangle {
    pub normal: Vec3,
    pub vertices: [Vec3; 3],
}

/// Parsed STL model.
#[derive(Debug)]
pub struct StlModel {
    pub name: String,
    pub triangles: Vec<StlTriangle>,
}

/// Read an STL model from a file path.
pub fn read_stl(path: &std::path::Path) -> Result<StlModel> {
    let data = std::fs::read(path)?;
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("STL Model")
        .to_string();
    read_stl_from_data(&data, &name)
}

/// Read an STL model from in-memory bytes.
pub fn read_stl_from_data(data: &[u8], name: &str) -> Result<StlModel> {
    if data.len() < 84 {
        // Below the minimum binary header size; try ASCII instead
        return parse_ascii(data, name);
    }

    // Binary length check: 84 + tri_count * 50 == data.len()
    let tri_count = {
        let mut cursor = Cursor::new(&data[80..84]);
        cursor.read_u32::<LittleEndian>().unwrap_or(0) as usize
    };

    let expected_len = 84usize.saturating_add(tri_count.saturating_mul(50));
    if expected_len == data.len() && tri_count > 0 {
        parse_binary(data, name, tri_count)
    } else {
        // Binary length mismatch; try as ASCII
        parse_ascii(data, name)
    }
}

fn parse_binary(data: &[u8], name: &str, tri_count: usize) -> Result<StlModel> {
    let expected = 84 + tri_count * 50;
    if data.len() < expected {
        return Err(PoponeError::StlParse(
            t!(
                "error.stl.binary_too_short",
                expected = expected.to_string(),
                actual = data.len().to_string()
            )
            .to_string(),
        ));
    }

    let mut triangles = Vec::with_capacity(tri_count);
    let mut cursor = Cursor::new(&data[84..]);

    for i in 0..tri_count {
        let normal = read_vec3(&mut cursor).map_err(|e| {
            PoponeError::StlParse(
                t!(
                    "error.stl.triangle_normal_failed",
                    index = i.to_string(),
                    detail = e.to_string()
                )
                .to_string(),
            )
        })?;
        let v0 = read_vec3(&mut cursor).map_err(|e| {
            PoponeError::StlParse(
                t!(
                    "error.stl.triangle_vertex_failed",
                    index = i.to_string(),
                    vertex = "0".to_string(),
                    detail = e.to_string()
                )
                .to_string(),
            )
        })?;
        let v1 = read_vec3(&mut cursor).map_err(|e| {
            PoponeError::StlParse(
                t!(
                    "error.stl.triangle_vertex_failed",
                    index = i.to_string(),
                    vertex = "1".to_string(),
                    detail = e.to_string()
                )
                .to_string(),
            )
        })?;
        let v2 = read_vec3(&mut cursor).map_err(|e| {
            PoponeError::StlParse(
                t!(
                    "error.stl.triangle_vertex_failed",
                    index = i.to_string(),
                    vertex = "2".to_string(),
                    detail = e.to_string()
                )
                .to_string(),
            )
        })?;
        // Skip attribute bytes (2 bytes)
        let _attr = cursor.read_u16::<LittleEndian>().map_err(|e| {
            PoponeError::StlParse(
                t!(
                    "error.stl.triangle_attr_failed",
                    index = i.to_string(),
                    detail = e.to_string()
                )
                .to_string(),
            )
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
    let text = std::str::from_utf8(data).map_err(|e| {
        PoponeError::StlParse(t!("error.stl.ascii_utf8_failed", detail = e.to_string()).to_string())
    })?;

    let mut triangles = Vec::new();
    let mut lines = text.lines().map(|l| l.trim());
    let mut model_name = name.to_string();

    // "solid <name>" header
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
            .ok_or_else(|| PoponeError::StlParse(t!("error.stl.outer_loop_missing").to_string()))?;
        if !loop_line.starts_with("outer loop") {
            return Err(PoponeError::StlParse(
                t!(
                    "error.stl.expect_actual",
                    expected = "outer loop".to_string(),
                    actual = loop_line.to_string()
                )
                .to_string(),
            ));
        }

        // vertex x y z (three of them)
        let mut verts = [Vec3::ZERO; 3];
        for v in &mut verts {
            let vline = lines.next().ok_or_else(|| {
                PoponeError::StlParse(t!("error.stl.vertex_line_missing").to_string())
            })?;
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
            t!("error.stl.no_triangles").to_string(),
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
        .ok_or_else(|| {
            PoponeError::StlParse(
                t!(
                    "error.stl.expect_actual",
                    expected = prefix.to_string(),
                    actual = line.to_string()
                )
                .to_string(),
            )
        })?
        .trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() < 3 {
        return Err(PoponeError::StlParse(
            t!("error.stl.vec3_parse_failed", rest = rest.to_string()).to_string(),
        ));
    }
    let parse_float = |s: &str| -> Result<f32> {
        s.parse::<f32>().map_err(|e| {
            PoponeError::StlParse(
                t!("error.stl.float_parse_failed", detail = e.to_string()).to_string(),
            )
        })
    };
    let x = parse_float(parts[0])?;
    let y = parse_float(parts[1])?;
    let z = parse_float(parts[2])?;
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
        // Minimal binary STL: 80-byte header + 0 triangles
        let mut data = vec![0u8; 84];
        // tri_count = 0
        data[80..84].copy_from_slice(&0u32.to_le_bytes());
        // tri_count == 0 with matching length, but empty -> falls through to ASCII -> error
        let result = read_stl_from_data(&data, "test");
        assert!(result.is_err());
    }

    #[test]
    fn parse_binary_stl_one_triangle() {
        let mut data = vec![0u8; 84 + 50];
        // tri_count = 1
        data[80..84].copy_from_slice(&1u32.to_le_bytes());
        // Normal (0, 0, 1)
        data[84..88].copy_from_slice(&0f32.to_le_bytes());
        data[88..92].copy_from_slice(&0f32.to_le_bytes());
        data[92..96].copy_from_slice(&1f32.to_le_bytes());
        // Vertex 0 (0, 0, 0)
        // Vertex 1 (1, 0, 0)
        data[108..112].copy_from_slice(&1f32.to_le_bytes());
        // Vertex 2 (0, 1, 0)
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
