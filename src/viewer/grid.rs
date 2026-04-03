use super::gpu::GridVertex;

/// モデルの bbox からグリッドの extent と step を計算する
/// デフォルト（extent=100, step=5）を下限とし、巨大モデルの場合のみ拡大する
pub fn compute_grid_params(bbox_min: glam::Vec3, bbox_max: glam::Vec3) -> (f32, f32) {
    const DEFAULT_EXTENT: f32 = 100.0;
    const DEFAULT_STEP: f32 = 5.0;

    let size = bbox_max - bbox_min;
    let max_dim = size.x.abs().max(size.y.abs()).max(size.z.abs());
    // モデル中心からのオフセットも考慮
    let center = (bbox_min + bbox_max) * 0.5;
    let max_offset = center.x.abs().max(center.z.abs());
    let needed = max_dim * 0.5 + max_offset;

    // デフォルト範囲内なら従来のグリッドをそのまま使用
    if needed <= DEFAULT_EXTENT {
        return (DEFAULT_EXTENT, DEFAULT_STEP);
    }

    // 巨大モデル: extent を切りの良い値に切り上げ
    let nice_values = [200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0];
    let extent = nice_values
        .iter()
        .find(|&&v| v >= needed)
        .copied()
        .unwrap_or(needed.ceil());

    // step: グリッド線を 40 本程度（片側20本）に保つ
    let raw_step = extent / 20.0;
    let nice_steps = [10.0, 20.0, 50.0, 100.0, 200.0, 500.0];
    let step = nice_steps
        .iter()
        .find(|&&v| v >= raw_step)
        .copied()
        .unwrap_or(raw_step.ceil());

    (extent, step)
}

/// グリッド床の頂点データを生成
/// PMX スケールで Y=0 平面に描画
pub fn build_grid_vertices() -> (Vec<GridVertex>, u32) {
    build_grid_vertices_with_params(100.0, 5.0)
}

/// 指定した extent / step でグリッド頂点を生成
pub fn build_grid_vertices_with_params(extent: f32, step: f32) -> (Vec<GridVertex>, u32) {
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

    // Y軸（緑っぽい）
    let axis_color_y = [0.3, 0.6, 0.3, 1.0];
    verts.push(GridVertex {
        position: [0.0, 0.0, 0.0],
        color: axis_color_y,
    });
    verts.push(GridVertex {
        position: [0.0, extent, 0.0],
        color: axis_color_y,
    });

    let count = verts.len() as u32;
    (verts, count)
}
