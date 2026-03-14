use super::gpu::GridVertex;

/// グリッド床の頂点データを生成
/// PMX スケールで Y=0 平面に描画
pub fn build_grid_vertices() -> (Vec<GridVertex>, u32) {
    let extent = 100.0_f32; // PMX単位
    let step = 5.0_f32;
    let lines_per_axis = (2.0 * extent / step) as usize + 1;
    let mut verts = Vec::with_capacity(lines_per_axis * 4);
    let color = [0.35, 0.35, 0.35, 1.0];
    let axis_color_x = [0.6, 0.3, 0.3, 1.0]; // X軸（赤っぽい）
    let axis_color_z = [0.3, 0.3, 0.6, 1.0]; // Z軸（青っぽい）

    let mut x = -extent;
    while x <= extent + 0.001 {
        let c = if x.abs() < 0.01 { axis_color_z } else { color };
        verts.push(GridVertex {
            position: [x, 0.0, -extent],
            color: c,
        });
        verts.push(GridVertex {
            position: [x, 0.0, extent],
            color: c,
        });
        x += step;
    }

    let mut z = -extent;
    while z <= extent + 0.001 {
        let c = if z.abs() < 0.01 { axis_color_x } else { color };
        verts.push(GridVertex {
            position: [-extent, 0.0, z],
            color: c,
        });
        verts.push(GridVertex {
            position: [extent, 0.0, z],
            color: c,
        });
        z += step;
    }

    let count = verts.len() as u32;
    (verts, count)
}
