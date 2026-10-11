//! A portable surface-of-revolution evaluator. No windowing, GPU or document dependencies.
//!
//! Open profiles optionally close their circular ends with caps. Faces are projected and sorted back to front,
//! then flat or smoothly shaded with vector gradients. This preview uses a painter's algorithm, not a depth buffer:
//! intersecting profiles can have ambiguous visibility and are not solid modelling inputs.
#![forbid(unsafe_code)]

mod visibility;
pub use visibility::{VisibleFace, visible_faces};
mod shading;
pub use shading::ShadeGradient;
mod caps;

use vectorcraft_geom::{BezPath, ParamCurve, PathData, Point, Rect, Vec2, kurbo::ParamCurveDeriv};

/// Resource limits shared by desktop, CLI and browser evaluation.
const MAX_PROFILE_POINTS: usize = 1024;
const MAX_FACES: usize = 32768;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct V3(f64, f64, f64);

impl V3 {
    fn sub(self, b: Self) -> Self {
        Self(self.0 - b.0, self.1 - b.1, self.2 - b.2)
    }
    fn cross(self, b: Self) -> Self {
        Self(self.1 * b.2 - self.2 * b.1, self.2 * b.0 - self.0 * b.2, self.0 * b.1 - self.1 * b.0)
    }
    fn dot(self, b: Self) -> f64 {
        self.0 * b.0 + self.1 * b.1 + self.2 * b.2
    }
    fn unit(self) -> Self {
        let l = self.dot(self).sqrt();
        if l < 1e-12 { Self::default() } else { Self(self.0 / l, self.1 / l, self.2 / l) }
    }
}

/// The vertical axis is offset outward from the selected edge of the profile's bounds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Edge {
    #[default]
    Left,
    Right,
}

/// All angles are degrees; offset is in document points, lighting values are 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Revolve {
    pub angle: f64,
    pub offset: f64,
    pub edge: Edge,
    pub rotation_x: f64,
    pub rotation_y: f64,
    pub rotation_z: f64,
    /// Perspective strength, 0 for orthographic projection, up to 1.
    pub perspective: f64,
    /// Radial subdivisions for a full turn, clamped to 8..128.
    pub segments: usize,
    pub light_azimuth: f64,
    pub light_elevation: f64,
    pub light_intensity: f64,
    pub ambient: f64,
    pub shade: bool,
    /// Interpolate profile normals using vector gradients. Defaults off for existing artwork.
    pub smooth_shading: bool,
    /// Close circular endpoint rings of open profiles. Partial sweeps get sectors;
    /// the angular cut sides are left open. Defaults off for existing artwork.
    pub cap_ends: bool,
}

impl Default for Revolve {
    fn default() -> Self {
        Self {
            angle: 360.0,
            offset: 0.0,
            edge: Edge::Left,
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: 0.0,
            perspective: 0.0,
            segments: 64,
            light_azimuth: -45.0,
            light_elevation: 45.0,
            light_intensity: 0.8,
            ambient: 0.25,
            shade: true,
            smooth_shading: false,
            cap_ends: false,
        }
    }
}

fn finite(v: f64, fallback: f64, min: f64, max: f64) -> f64 {
    if v.is_finite() { v.clamp(min, max) } else { fallback }
}

impl Revolve {
    /// Unit bases of the rings that adjust the Euler X, Y and Z view parameters.
    /// X turns before Y and Z, Y before Z, and Z turns in the screen plane.
    /// Each pair has a cross product pointing along that parameter's rotation axis.
    pub fn rotation_planes(self) -> [[[f64; 3]; 2]; 3] {
        let o = self.sanitized();
        let x_frame = Self { rotation_x: 0.0, ..o };
        let y_frame = Self { rotation_x: 0.0, rotation_y: 0.0, ..o };
        [
            [x_frame.rotate(V3(0.0, 1.0, 0.0)), x_frame.rotate(V3(0.0, 0.0, 1.0))],
            [y_frame.rotate(V3(1.0, 0.0, 0.0)), y_frame.rotate(V3(0.0, 0.0, -1.0))],
            [V3(1.0, 0.0, 0.0), V3(0.0, 1.0, 0.0)],
        ]
        .map(|plane| plane.map(|v| [v.0, v.1, v.2]))
    }

    /// The fixed centre of view rotation, in document coordinates.
    pub fn rotation_origin(self, bounds: Rect) -> Point {
        let o = self.sanitized();
        Point::new(if o.edge == Edge::Left { bounds.x0 - o.offset } else { bounds.x1 + o.offset }, bounds.center().y)
    }

    /// Rotated unit axes for a view-orientation guide (screen Y points down).
    pub fn view_axes(self) -> [Point; 3] {
        [V3(1.0, 0.0, 0.0), V3(0.0, 1.0, 0.0), V3(0.0, 0.0, 1.0)].map(|v| {
            let p = self.sanitized().rotate(v);
            Point::new(p.0, p.1)
        })
    }

    /// The same projected vertical axis used by the surface evaluator.
    pub fn projected_axis(self, bounds: Rect) -> [Point; 2] {
        let o = self.sanitized();
        let axis = match o.edge {
            Edge::Left => bounds.x0 - o.offset,
            Edge::Right => bounds.x1 + o.offset,
        };
        let center_y = bounds.center().y;
        let radius = (bounds.x0 - axis).abs().max((bounds.x1 - axis).abs());
        let distance = 4.0 * radius.max(bounds.height().abs()).max(1.0);
        [bounds.y0, bounds.y1].map(|y| {
            let v = o.rotate(V3(0.0, y - center_y, 0.0));
            let factor = distance / (distance - v.2 * o.perspective);
            Point::new(axis + v.0 * factor, center_y + v.1 * factor)
        })
    }

    /// Directional light in view space, shared by shading and interactive guides.
    pub fn light_direction(self) -> [f64; 3] {
        let o = self.sanitized();
        let (sl, cl) = o.light_elevation.to_radians().sin_cos();
        let (sa, ca) = o.light_azimuth.to_radians().sin_cos();
        [cl * sa, -sl, cl * ca]
    }

    /// Light-direction orbits as (centre, cosine basis, sine basis), in view space.
    /// The first changes azimuth at the current elevation. The second changes elevation
    /// at the current azimuth. The meridian's valid parameter range is -90..90 degrees.
    pub fn light_orbits(self) -> [[[f64; 3]; 3]; 2] {
        let o = self.sanitized();
        let (se, ce) = o.light_elevation.to_radians().sin_cos();
        let (sa, ca) = o.light_azimuth.to_radians().sin_cos();
        [[[0.0, -se, 0.0], [0.0, 0.0, ce], [ce, 0.0, 0.0]], [[0.0, 0.0, 0.0], [sa, 0.0, ca], [0.0, -1.0, 0.0]]]
    }

    /// Move a light on its current hemisphere, preserving the azimuth at the poles.
    /// A light behind the object stays behind when its projected handle is dragged.
    pub fn light_at_on_side(self, point: Point) -> [f64; 2] {
        let o = self.sanitized();
        let point = Point::new(finite(point.x, 0.0, -1e8, 1e8), finite(point.y, 0.0, -1e8, 1e8));
        let length = point.x.hypot(point.y).max(1.0);
        let (x, y) = (point.x / length, point.y / length);
        let z = (1.0 - x * x - y * y).max(0.0).sqrt().copysign(o.light_direction()[2]);
        let azimuth = if x.hypot(z) < 1e-9 { o.light_azimuth } else { x.atan2(z).to_degrees() };
        [azimuth, (-y).clamp(-1.0, 1.0).asin().to_degrees()]
    }

    /// A dragged light position on the front hemisphere, in normalized view coordinates.
    pub fn light_at(point: Point) -> [f64; 2] {
        let length = point.x.hypot(point.y).max(1.0);
        let (x, y) = (point.x / length, point.y / length);
        let z = (1.0 - x * x - y * y).max(0.0).sqrt();
        [x.atan2(z).to_degrees(), (-y).clamp(-1.0, 1.0).asin().to_degrees()]
    }

    fn sanitized(self) -> Self {
        Self {
            angle: finite(self.angle, 360.0, 0.0, 360.0),
            offset: finite(self.offset, 0.0, 0.0, 1e5),
            rotation_x: finite(self.rotation_x, 0.0, -360.0, 360.0),
            rotation_y: finite(self.rotation_y, 0.0, -360.0, 360.0),
            rotation_z: finite(self.rotation_z, 0.0, -360.0, 360.0),
            perspective: finite(self.perspective, 0.0, 0.0, 1.0),
            segments: self.segments.clamp(8, 128),
            light_azimuth: finite(self.light_azimuth, -45.0, -360.0, 360.0),
            light_elevation: finite(self.light_elevation, 45.0, -90.0, 90.0),
            light_intensity: finite(self.light_intensity, 0.8, 0.0, 1.0),
            ambient: finite(self.ambient, 0.25, 0.0, 1.0),
            ..self
        }
    }
    fn rotate(self, v: V3) -> V3 {
        let (sx, cx) = self.rotation_x.to_radians().sin_cos();
        let (sy, cy) = self.rotation_y.to_radians().sin_cos();
        let (sz, cz) = self.rotation_z.to_radians().sin_cos();
        let v = V3(v.0, cx * v.1 - sx * v.2, sx * v.1 + cx * v.2);
        let v = V3(cy * v.0 + sy * v.2, v.1, -sy * v.0 + cy * v.2);
        V3(cz * v.0 - sz * v.1, sz * v.0 + cz * v.1, v.2)
    }
}

/// One projected vector face. Brightness multiplies the source colour; an optional
/// gradient replaces that constant brightness for a smoothly shaded triangle.
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub points: Vec<Point>,
    pub depth: f64,
    pub brightness: f64,
    pub gradient: Option<ShadeGradient>,
    /// Curved vector boundary; points remain a bounded visibility approximation.
    pub outline: Option<PathData>,
    /// A single endpoint disk or sector, with its axis centre first in points.
    pub cap: bool,
    /// Bounded radial strips for a body face joining two curved endpoint rings.
    pub visibility_pieces: Option<Vec<Vec<Point>>>,
}

impl Face {
    pub fn path(&self) -> PathData {
        if let Some(path) = &self.outline {
            return path.clone();
        }
        let mut bp = BezPath::new();
        if let Some(first) = self.points.first() {
            bp.move_to(*first);
            for p in self.points.iter().skip(1) {
                bp.line_to(*p);
            }
            bp.close_path();
        }
        PathData::from_bezpath(&bp)
    }
}

/// Body faces and a cap's planar seed use identical two-sided shading and projection.
fn project_face(vertices: &[V3], normals: Option<&[V3]>, o: Revolve, origin: Point, distance: f64, light: V3) -> Result<Option<Face>, String> {
    let Some(first) = vertices.first() else { return Ok(None) };
    // A triangle fan also handles a quad whose first edge lies on the axis.
    let mut normal = vertices
        .get(1..)
        .unwrap_or_default()
        .windows(2)
        .filter_map(|pair| {
            let [a, b] = pair else { return None };
            Some(a.sub(*first).cross(b.sub(*first)).unit())
        })
        .find(|n| n.dot(*n) >= 0.5)
        .unwrap_or_default();
    if normal.dot(normal) < 0.5 {
        return Ok(None);
    }
    // Profiles may run up or down. Shade the visible side of this two-sided surface.
    if normal.2 < 0.0 {
        normal = V3(-normal.0, -normal.1, -normal.2);
    }
    let brightness = if o.shade { (o.ambient + o.light_intensity * normal.dot(light).max(0.0)).clamp(0.0, 1.0) } else { 1.0 };
    let points: Vec<_> = vertices
        .iter()
        .map(|v| {
            let factor = distance / (distance - v.2 * o.perspective);
            Point::new(origin.x + v.0 * factor, origin.y + v.1 * factor)
        })
        .collect();
    if points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return Err("Revolve projection is not finite".into());
    }
    let area = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| (a.x - origin.x) * (b.y - origin.y) - (b.x - origin.x) * (a.y - origin.y))
        .sum::<f64>();
    if area.abs() < 1e-10 {
        return Ok(None);
    }
    let (brightness, gradient) = if let Some(normals) = normals {
        let values: Vec<_> = normals
            .iter()
            .map(|n| {
                let n = if n.dot(normal) < 0.0 { V3(-n.0, -n.1, -n.2) } else { *n };
                (o.ambient + o.light_intensity * n.dot(light).max(0.0)).clamp(0.0, 1.0)
            })
            .collect();
        let brightness = if values.is_empty() { brightness } else { values.iter().sum::<f64>() / values.len() as f64 };
        (brightness, ShadeGradient::triangle(&points, &values))
    } else {
        (brightness, None)
    };
    Ok(Some(Face {
        points,
        depth: vertices.iter().map(|v| v.2).sum::<f64>() / vertices.len() as f64,
        brightness,
        gradient,
        outline: None,
        cap: false,
        visibility_pieces: None,
    }))
}

/// Evaluate a profile in document space. All subpaths share one axis from `bounds`.
/// Invalid or excessively complex inputs return an error, never a partial surface.
pub fn revolve(path: &PathData, bounds: Rect, options: Revolve) -> Result<Vec<Face>, String> {
    let o = options.sanitized();
    if ![bounds.x0, bounds.y0, bounds.x1, bounds.y1].iter().all(|v| v.is_finite() && v.abs() <= 1e8) {
        return Err("Revolve requires finite profile coordinates within 100 million points".into());
    }
    let axis = match o.edge {
        Edge::Left => bounds.x0 - o.offset,
        Edge::Right => bounds.x1 + o.offset,
    };
    let center_y = (bounds.y0 + bounds.y1) * 0.5;
    let radius = (bounds.x0 - axis).abs().max((bounds.x1 - axis).abs());
    let distance = 4.0 * radius.max(bounds.height().abs()).max(1.0);
    let steps = ((o.segments as f64 * o.angle / 360.0).ceil() as usize).max(1);
    let sweep = o.angle.to_radians();
    let smooth = o.shade && o.smooth_shading;
    let [lx, ly, lz] = o.light_direction();
    let light = V3(lx, ly, lz);
    let mut faces = Vec::new();
    let vertex = |p: Point, step: usize| {
        let (s, c) = (sweep * step as f64 / steps as f64).sin_cos();
        o.rotate(V3((p.x - axis) * c, p.y - center_y, (p.x - axis) * s))
    };
    let normal = |tangent: Vec2, step: usize| {
        let (s, c) = (sweep * step as f64 / steps as f64).sin_cos();
        o.rotate(V3(tangent.y * c, -tangent.x, tangent.y * s).unit())
    };
    let mut mesh_cost = 0;
    let mut emit = |face: Option<Face>, cost: usize| -> Result<(), String> {
        if let Some(face) = face {
            faces.push(face);
            mesh_cost += cost;
            if mesh_cost > MAX_FACES {
                return Err("Revolve surface is too complex; reduce Segments or simplify the profile".into());
            }
        }
        Ok(())
    };
    let mut count = 0;
    let mut curve_budget = 0;
    for sp in &path.subpaths {
        let mut profile = Vec::new();
        let mut tangents = Vec::new();
        if let Some(first) = sp.anchors.first() {
            profile.push(first.p);
            count += 1;
            if count > MAX_PROFILE_POINTS {
                return Err("Revolve profile is too complex".into());
            }
        }
        let segments = if sp.closed { sp.anchors.len() } else { sp.anchors.len().saturating_sub(1) };
        // Bound work before sampling any data. Curves get sixteen samples, straight edges one.
        if segments > MAX_PROFILE_POINTS {
            return Err("Revolve profile is too complex".into());
        }
        for i in 0..segments {
            let curve = sp.segment(i);
            let samples = if sp.segment_is_line(i) { 1 } else { 16 };
            for k in 1..=samples {
                let t0 = (k - 1) as f64 / samples as f64;
                let t1 = k as f64 / samples as f64;
                let b = curve.eval(t1);
                let tangent = |t: f64| {
                    if !smooth {
                        return Vec2::ZERO;
                    }
                    let a = curve.eval(t0);
                    if samples == 1 {
                        return b - a;
                    }
                    let d = curve.deriv();
                    let v = d.eval(t).to_vec2();
                    if v.hypot2() > 1e-20 {
                        return v;
                    }
                    // Repeated endpoint handles still have a one-sided curve tangent.
                    let v = d.eval(t.clamp(1e-7, 1.0 - 1e-7)).to_vec2();
                    if v.hypot2() > 1e-20 { v } else { b - a }
                };
                tangents.push([tangent(t0), tangent(t1)]);
                profile.push(b);
                count += 1;
                if count > MAX_PROFILE_POINTS {
                    return Err("Revolve profile is too complex".into());
                }
            }
        }
        let ends = o.cap_ends && !sp.closed && profile.len() >= 2 && profile.first().zip(profile.last()).is_some_and(|(a, b)| a.distance(*b) > 1e-10);
        let mut rings = [None, None];
        if ends {
            for (ring, end) in rings.iter_mut().zip([profile.first(), profile.last()]) {
                if let Some(end) = end
                    && (end.x - axis).abs() > 1e-10
                {
                    *ring = Some(caps::Ring::new(*end, bounds, o, steps, &mut curve_budget)?);
                }
            }
        }
        for (i, (pair, [ta, tb])) in profile.windows(2).zip(tangents).enumerate() {
            let [a, b] = pair else { continue };
            if ![a.x, a.y, b.x, b.y].iter().all(|v| v.is_finite() && v.abs() <= 1e8) {
                return Err("Revolve profile contains invalid coordinates".into());
            }
            for j in 0..steps {
                let [v0, v1, v2, v3] = [vertex(*a, j), vertex(*b, j), vertex(*b, j + 1), vertex(*a, j + 1)];
                let first = (i == 0).then(|| rings.first().and_then(Option::as_ref).and_then(|r| r.arcs.get(j))).flatten();
                let last = (i + 2 == profile.len()).then(|| rings.last().and_then(Option::as_ref).and_then(|r| r.arcs.get(j))).flatten();
                let projected = |vertices: &[V3], normals: Option<&[V3]>, edges: &[(usize, &caps::RingArc, bool)]| {
                    let mut face = project_face(vertices, normals, o, Point::new(axis, center_y), distance, light)?;
                    if let Some(face) = &mut face {
                        caps::curve_edges(face, edges);
                    }
                    Ok::<_, String>(face)
                };
                if smooth {
                    let [n0, n1, n2, n3] = [normal(ta, j), normal(tb, j), normal(tb, j + 1), normal(ta, j + 1)];
                    emit(projected(&[v0, v1, v2], Some(&[n0, n1, n2]), &last.map(|arc| (1, arc, false)).into_iter().collect::<Vec<_>>())?, 1)?;
                    emit(projected(&[v0, v2, v3], Some(&[n0, n2, n3]), &first.map(|arc| (2, arc, true)).into_iter().collect::<Vec<_>>())?, 1)?;
                } else {
                    let edges: Vec<_> = last.map(|arc| (1, arc, false)).into_iter().chain(first.map(|arc| (3, arc, true))).collect();
                    emit(projected(&[v0, v1, v2, v3], None, &edges)?, 1)?;
                }
            }
        }
        for (index, ring) in rings.iter().enumerate().filter_map(|(i, r)| r.as_ref().map(|r| (i, r))) {
            let neighbours =
                if index == 0 { profile.first().zip(profile.get(1)) } else { profile.last().zip(profile.get(profile.len().saturating_sub(2))) };
            if let Some((end, neighbour)) = neighbours {
                let dy = end.y - neighbour.y;
                let outward = if dy.abs() > 1e-10 {
                    dy.signum()
                } else if end.y < center_y {
                    -1.0
                } else {
                    1.0
                };
                // Keep the original tessellation work budget, even though each cap
                // now produces only one curved vector face.
                emit(ring.face(bounds, o, light, outward)?, steps)?;
            }
        }
    }
    faces.sort_by(|a, b| a.depth.total_cmp(&b.depth));
    Ok(faces)
}

#[cfg(test)]
mod tests_caps;
#[cfg(test)]
mod tests_smooth;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn light_orbits_and_handle_projection_match_the_shading_direction_on_both_sides() {
        for azimuth in [-170.0f64, -45.0, 0.0, 83.0, 167.0] {
            for elevation in [-85.0f64, -30.0, 0.0, 45.0, 90.0] {
                let o = Revolve { light_azimuth: azimuth, light_elevation: elevation, ..Default::default() };
                let direction = o.light_direction();
                for ([center, u, v], angle) in o.light_orbits().into_iter().zip([azimuth, elevation]) {
                    let (s, c) = angle.to_radians().sin_cos();
                    for i in 0..3 {
                        assert!((center[i] + u[i] * c + v[i] * s - direction[i]).abs() < 1e-12);
                    }
                }
                let [a, e] = o.light_at_on_side(Point::new(direction[0], direction[1]));
                assert!((a - azimuth).abs() < 1e-9 && (e - elevation).abs() < 1e-9, "{azimuth}, {elevation} -> {a}, {e}");
            }
        }
        let o = Revolve { light_azimuth: 135.0, ..Default::default() };
        let [azimuth, elevation] = o.light_at_on_side(Point::new(0.3, -0.4));
        let [x, y, z] = Revolve { light_azimuth: azimuth, light_elevation: elevation, ..o }.light_direction();
        assert!((x - 0.3).abs() < 1e-12 && (y + 0.4).abs() < 1e-12 && z < 0.0);
        let [a, e] = o.light_at_on_side(Point::new(f64::NAN, f64::INFINITY));
        assert!(a.is_finite() && e.is_finite());
    }

    #[test]
    fn ring_axes_match_euler_parameter_rotation_and_origin_stays_fixed() {
        let o = Revolve { rotation_x: 27.0, rotation_y: -39.0, rotation_z: 53.0, ..Default::default() };
        let seed = V3(0.3, -0.5, 0.7);
        let before = o.rotate(seed);
        let planes = o.rotation_planes();
        for (axis, [u, v]) in planes.iter().enumerate() {
            let normal = V3(u[0], u[1], u[2]).cross(V3(v[0], v[1], v[2]));
            assert!((normal.dot(normal) - 1.0).abs() < 1e-12);
            let mut changed = o;
            match axis {
                0 => changed.rotation_x += 0.0001,
                1 => changed.rotation_y += 0.0001,
                _ => changed.rotation_z += 0.0001,
            }
            let actual = changed.rotate(seed).sub(before);
            let expected = normal.cross(before);
            let step = 0.0001f64.to_radians();
            for (a, b) in [(actual.0 / step, expected.0), (actual.1 / step, expected.1), (actual.2 / step, expected.2)] {
                assert!((a - b).abs() < 1e-5);
            }
        }
        assert_eq!(Revolve { edge: Edge::Right, offset: 12.0, ..o }.rotation_origin(Rect::new(20.0, 30.0, 80.0, 100.0)), Point::new(92.0, 65.0));
    }
    #[test]
    fn guides_match_projection_and_dragged_light_matches_shading_direction() {
        let o = Revolve { rotation_x: 0.0, rotation_y: 0.0, rotation_z: 0.0, offset: 10.0, ..Default::default() };
        assert_eq!(o.view_axes(), [Point::new(1.0, 0.0), Point::new(0.0, 1.0), Point::new(0.0, 0.0)]);
        assert_eq!(o.projected_axis(Rect::new(20.0, 30.0, 80.0, 100.0)), [Point::new(10.0, 30.0), Point::new(10.0, 100.0)]);
        let [azimuth, elevation] = Revolve::light_at(Point::new(0.3, -0.4));
        let [x, y, z] = Revolve { light_azimuth: azimuth, light_elevation: elevation, ..o }.light_direction();
        assert!((x - 0.3).abs() < 1e-9 && (y + 0.4).abs() < 1e-9 && z > 0.0);
        let [azimuth, elevation] = Revolve::light_at(Point::new(2.0, -2.0));
        assert!(azimuth.is_finite() && elevation.is_finite() && elevation.abs() <= 90.0);
    }
    fn cylinder() -> (PathData, Rect) {
        let mut bp = BezPath::new();
        bp.move_to((40.0, 10.0));
        bp.line_to((40.0, 90.0));
        (PathData::from_bezpath(&bp), Rect::new(0.0, 10.0, 40.0, 90.0))
    }
    #[test]
    fn cylinder_has_expected_silhouette_depth_order_and_shading() {
        let (p, b) = cylinder();
        let faces = revolve(&p, b, Revolve { rotation_x: 0.0, rotation_y: 0.0, ..Default::default() }).unwrap();
        let points: Vec<_> = faces.iter().flat_map(|f| &f.points).collect();
        assert!((points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min) + 40.0).abs() < 1e-6);
        assert!((points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max) - 40.0).abs() < 1e-6);
        assert!(faces.windows(2).all(|w| w[0].depth <= w[1].depth));
        assert!(faces.iter().any(|f| f.brightness > 0.8) && faces.iter().any(|f| f.brightness < 0.4));
    }
    #[test]
    fn edge_offset_sweep_and_rotation_change_the_surface() {
        let (p, b) = cylinder();
        let base = revolve(&p, b, Revolve::default()).unwrap();
        for o in [
            Revolve { edge: Edge::Right, offset: 20.0, ..Default::default() },
            Revolve { angle: 120.0, ..Default::default() },
            Revolve { rotation_z: 60.0, perspective: 0.8, ..Default::default() },
        ] {
            assert_ne!(base, revolve(&p, b, o).unwrap());
        }
        assert!(revolve(&p, b, Revolve { angle: 0.0, ..Default::default() }).unwrap().is_empty());
    }
    #[test]
    fn malformed_and_degenerate_inputs_are_bounded() {
        let (p, b) = cylinder();
        let f = revolve(&p, b, Revolve { segments: usize::MAX, angle: f64::NAN, offset: f64::INFINITY, ..Default::default() }).unwrap();
        assert!(f.len() <= 128);
        assert!(revolve(&p, Rect::new(f64::NAN, 0.0, 1.0, 1.0), Revolve::default()).is_err());
        assert!(revolve(&PathData::default(), b, Revolve::default()).unwrap().is_empty());
    }
}
