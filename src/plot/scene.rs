//! 场景 IR：与后端无关的矢量图元。
//!
//! 图表先构建成 `Scene`，再由 raster / SVG / PDF 三个后端消费。
//! 文字在进入场景前已经转成路径，所以三种出口的版式完全一致。

use crate::core::color::Rgba;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

impl Pt {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn lerp(self, other: Pt, t: f32) -> Pt {
        Pt::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn center(&self) -> Pt {
        Pt::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    pub fn inset(&self, d: f32) -> Rect {
        Rect::new(
            self.x + d,
            self.y + d,
            (self.w - 2.0 * d).max(0.0),
            (self.h - 2.0 * d).max(0.0),
        )
    }
    pub fn inset_xy(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(
            self.x + dx,
            self.y + dy,
            (self.w - 2.0 * dx).max(0.0),
            (self.h - 2.0 * dy).max(0.0),
        )
    }
    pub fn contains(&self, p: Pt) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

#[derive(Clone, Debug)]
pub struct Stroke {
    pub width: f32,
    pub color: Rgba,
    pub dash: Vec<f32>,
    pub cap: LineCap,
    pub join: LineJoin,
}

impl Stroke {
    pub fn new(width: f32, color: Rgba) -> Self {
        Self {
            width,
            color,
            dash: Vec::new(),
            cap: LineCap::Butt,
            join: LineJoin::Miter,
        }
    }
    pub fn round(mut self) -> Self {
        self.cap = LineCap::Round;
        self.join = LineJoin::Round;
        self
    }
    pub fn dash(mut self, pattern: &[f32]) -> Self {
        self.dash = pattern.to_vec();
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    Move(Pt),
    Line(Pt),
    Quad(Pt, Pt),
    Cubic(Pt, Pt, Pt),
    Close,
}

/// 一个复合路径（可含多条子路径）。
#[derive(Clone, Debug, Default)]
pub struct Path {
    pub segs: Vec<Seg>,
}

impl Path {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn is_empty(&self) -> bool {
        self.segs.is_empty()
    }
    pub fn move_to(&mut self, p: Pt) {
        self.segs.push(Seg::Move(p));
    }
    pub fn line_to(&mut self, p: Pt) {
        self.segs.push(Seg::Line(p));
    }
    pub fn quad_to(&mut self, c: Pt, p: Pt) {
        self.segs.push(Seg::Quad(c, p));
    }
    pub fn cubic_to(&mut self, c1: Pt, c2: Pt, p: Pt) {
        self.segs.push(Seg::Cubic(c1, c2, p));
    }
    pub fn close(&mut self) {
        self.segs.push(Seg::Close);
    }
    pub fn line(a: Pt, b: Pt) -> Self {
        let mut p = Self::new();
        p.move_to(a);
        p.line_to(b);
        p
    }
    pub fn rect(r: Rect) -> Self {
        let mut p = Self::new();
        p.move_to(Pt::new(r.x, r.y));
        p.line_to(Pt::new(r.right(), r.y));
        p.line_to(Pt::new(r.right(), r.bottom()));
        p.line_to(Pt::new(r.x, r.bottom()));
        p.close();
        p
    }
    pub fn circle(c: Pt, r: f32) -> Self {
        let k = 0.552_284_8 * r;
        let mut p = Self::new();
        p.move_to(Pt::new(c.x + r, c.y));
        p.cubic_to(
            Pt::new(c.x + r, c.y + k),
            Pt::new(c.x + k, c.y + r),
            Pt::new(c.x, c.y + r),
        );
        p.cubic_to(
            Pt::new(c.x - k, c.y + r),
            Pt::new(c.x - r, c.y + k),
            Pt::new(c.x - r, c.y),
        );
        p.cubic_to(
            Pt::new(c.x - r, c.y - k),
            Pt::new(c.x - k, c.y - r),
            Pt::new(c.x, c.y - r),
        );
        p.cubic_to(
            Pt::new(c.x + k, c.y - r),
            Pt::new(c.x + r, c.y - k),
            Pt::new(c.x + r, c.y),
        );
        p.close();
        p
    }
    pub fn polygon(points: &[Pt]) -> Self {
        let mut p = Self::new();
        if let Some(first) = points.first() {
            p.move_to(*first);
            for pt in &points[1..] {
                p.line_to(*pt);
            }
            p.close();
        }
        p
    }
    pub fn transform(&mut self, t: &Transform) {
        for seg in &mut self.segs {
            match seg {
                Seg::Move(p) | Seg::Line(p) => *p = t.apply(*p),
                Seg::Quad(c, p) => {
                    *c = t.apply(*c);
                    *p = t.apply(*p);
                }
                Seg::Cubic(c1, c2, p) => {
                    *c1 = t.apply(*c1);
                    *c2 = t.apply(*c2);
                    *p = t.apply(*p);
                }
                Seg::Close => {}
            }
        }
    }
    pub fn extend(&mut self, other: &Path) {
        self.segs.extend(other.segs.iter().copied());
    }
    pub fn bounds(&self) -> Option<Rect> {
        let mut min = (f32::INFINITY, f32::INFINITY);
        let mut max = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut any = false;
        let mut add = |p: Pt| {
            any = true;
            min.0 = min.0.min(p.x);
            min.1 = min.1.min(p.y);
            max.0 = max.0.max(p.x);
            max.1 = max.1.max(p.y);
        };
        for seg in &self.segs {
            match seg {
                Seg::Move(p) | Seg::Line(p) => add(*p),
                Seg::Quad(a, b) => {
                    add(*a);
                    add(*b);
                }
                Seg::Cubic(a, b, c) => {
                    add(*a);
                    add(*b);
                    add(*c);
                }
                Seg::Close => {}
            }
        }
        any.then(|| Rect::new(min.0, min.1, max.0 - min.0, max.1 - min.1))
    }
}

/// 2D 仿射变换：x' = a x + c y + e, y' = b x + d y + f
#[derive(Clone, Copy, Debug)]
pub struct Transform {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Transform {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };
    pub fn translate(x: f32, y: f32) -> Self {
        Self {
            e: x,
            f: y,
            ..Self::IDENTITY
        }
    }
    pub fn rotate(radians: f32) -> Self {
        let (s, c) = radians.sin_cos();
        Self {
            a: c,
            b: s,
            c: -s,
            d: c,
            e: 0.0,
            f: 0.0,
        }
    }
    pub fn around(origin: Pt, radians: f32) -> Self {
        let r = Self::rotate(radians);
        let t = Self::translate(origin.x, origin.y);
        let ti = Self::translate(-origin.x, -origin.y);
        t.then(&r).then(&ti)
    }
    pub fn then(&self, rhs: &Self) -> Self {
        // self * rhs（先应用 rhs）
        Self {
            a: self.a * rhs.a + self.c * rhs.b,
            b: self.b * rhs.a + self.d * rhs.b,
            c: self.a * rhs.c + self.c * rhs.d,
            d: self.b * rhs.c + self.d * rhs.d,
            e: self.a * rhs.e + self.c * rhs.f + self.e,
            f: self.b * rhs.e + self.d * rhs.f + self.f,
        }
    }
    pub fn apply(&self, p: Pt) -> Pt {
        Pt::new(
            self.a * p.x + self.c * p.y + self.e,
            self.b * p.x + self.d * p.y + self.f,
        )
    }
}

/// 一个绘制操作。
#[derive(Clone, Debug)]
pub struct Op {
    pub path: Path,
    pub fill: Option<Rgba>,
    pub stroke: Option<Stroke>,
}

#[derive(Clone, Debug)]
pub struct Scene {
    pub width: f32,
    pub height: f32,
    pub bg: Rgba,
    pub ops: Vec<Op>,
}

impl Scene {
    pub fn new(width: f32, height: f32, bg: Rgba) -> Self {
        Self {
            width,
            height,
            bg,
            ops: Vec::new(),
        }
    }

    pub fn push(&mut self, path: Path, fill: Option<Rgba>, stroke: Option<Stroke>) {
        if path.is_empty() && fill.is_none() {
            return;
        }
        self.ops.push(Op { path, fill, stroke });
    }

    pub fn fill(&mut self, path: Path, color: Rgba) {
        self.push(path, Some(color), None);
    }

    pub fn stroke(&mut self, path: Path, stroke: Stroke) {
        self.push(path, None, Some(stroke));
    }

    pub fn fill_stroke(&mut self, path: Path, fill: Rgba, stroke: Stroke) {
        self.push(path, Some(fill), Some(stroke));
    }

    pub fn rect(&mut self, r: Rect, fill: Option<Rgba>, stroke: Option<Stroke>) {
        self.push(Path::rect(r), fill, stroke);
    }

    pub fn line(&mut self, a: Pt, b: Pt, stroke: Stroke) {
        self.push(Path::line(a, b), None, Some(stroke));
    }

    pub fn circle(&mut self, c: Pt, r: f32, fill: Rgba) {
        self.push(Path::circle(c, r), Some(fill), None);
    }

    pub fn circle_stroke(&mut self, c: Pt, r: f32, stroke: Stroke) {
        self.push(Path::circle(c, r), None, Some(stroke));
    }
}
