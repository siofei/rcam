//! Outward-rounded supporting-curve intersections. The source rounding band
//! is an input; a nonpositive discriminant cannot certify a transverse cut.
use super::*;

#[derive(Clone, Copy)]
struct Interval {
    lo: f64,
    hi: f64,
}
impl Interval {
    fn exact(x: f64) -> Self {
        Self { lo: x, hi: x }
    }
    fn around(x: f64, e: f64) -> Self {
        Self {
            lo: (x - e).next_down(),
            hi: (x + e).next_up(),
        }
    }
    fn add(self, b: Self) -> Self {
        Self {
            lo: (self.lo + b.lo).next_down(),
            hi: (self.hi + b.hi).next_up(),
        }
    }
    fn neg(self) -> Self {
        Self {
            lo: -self.hi,
            hi: -self.lo,
        }
    }
    fn sub(self, b: Self) -> Self {
        self.add(b.neg())
    }
    fn mul(self, b: Self) -> Self {
        let p = [
            self.lo * b.lo,
            self.lo * b.hi,
            self.hi * b.lo,
            self.hi * b.hi,
        ];
        Self {
            lo: p.into_iter().fold(f64::INFINITY, f64::min).next_down(),
            hi: p.into_iter().fold(f64::NEG_INFINITY, f64::max).next_up(),
        }
    }
    fn square(self) -> Self {
        let p = self.lo * self.lo;
        let q = self.hi * self.hi;
        Self {
            lo: if self.lo <= 0. && self.hi >= 0. {
                0.
            } else {
                p.min(q).next_down().max(0.)
            },
            hi: p.max(q).next_up(),
        }
    }
    fn div(self, b: Self) -> Result<Self, QueryError> {
        if !b.lo.is_finite() || !b.hi.is_finite() || (b.lo <= 0. && b.hi >= 0.) {
            return Err(uncertain(
                "intersection denominator not separated from zero",
            ));
        }
        Ok(self.mul(Self {
            lo: (1. / b.hi).next_down(),
            hi: (1. / b.lo).next_up(),
        }))
    }
    fn sqrt(self) -> Result<Self, QueryError> {
        if !self.lo.is_finite() || !self.hi.is_finite() || self.lo <= 0. {
            return Err(uncertain("intersection discriminant not provably positive"));
        }
        Ok(Self {
            lo: self.lo.sqrt().next_down(),
            hi: self.hi.sqrt().next_up(),
        })
    }
    fn contains(self, x: f64) -> bool {
        self.lo <= x && x <= self.hi
    }
}
#[derive(Clone, Copy)]
struct PointBox {
    x: Interval,
    y: Interval,
}
impl PointBox {
    fn around(p: MmPoint, e: f64) -> Self {
        Self {
            x: Interval::around(p.x_mm, e),
            y: Interval::around(p.y_mm, e),
        }
    }
    fn sub(self, b: Self) -> Self {
        Self {
            x: self.x.sub(b.x),
            y: self.y.sub(b.y),
        }
    }
    fn add(self, b: Self) -> Self {
        Self {
            x: self.x.add(b.x),
            y: self.y.add(b.y),
        }
    }
    fn scale(self, b: Interval) -> Self {
        Self {
            x: self.x.mul(b),
            y: self.y.mul(b),
        }
    }
    fn dot(self, b: Self) -> Interval {
        self.x.mul(b.x).add(self.y.mul(b.y))
    }
    fn cross(self, b: Self) -> Interval {
        self.x.mul(b.y).sub(self.y.mul(b.x))
    }
    fn norm(self) -> Result<Interval, QueryError> {
        self.x.square().add(self.y.square()).sqrt()
    }
    fn divide(self, b: Interval) -> Result<Self, QueryError> {
        Ok(Self {
            x: self.x.div(b)?,
            y: self.y.div(b)?,
        })
    }
    fn contains(self, p: MmPoint) -> bool {
        self.x.contains(p.x_mm) && self.y.contains(p.y_mm)
    }
    fn radius_from(self, p: MmPoint) -> f64 {
        let x = (p.x_mm - self.x.lo)
            .abs()
            .max((self.x.hi - p.x_mm).abs())
            .next_up();
        let y = (p.y_mm - self.y.lo)
            .abs()
            .max((self.y.hi - p.y_mm).abs())
            .next_up();
        ((x * x).next_up() + (y * y).next_up())
            .next_up()
            .sqrt()
            .next_up()
    }
}

pub(super) fn position_error(
    a: &RegionEdge,
    b: &RegionEdge,
    p: MmPoint,
    e: f64,
) -> Result<f64, QueryError> {
    let boxes = match (a, b) {
        (RegionEdge::Line { start: a, end: b }, RegionEdge::Line { start: c, end: d }) => {
            // Collinear cuts are GIVEN endpoints, not division by a zero determinant.
            if (cross(*a, *b, *c) == 0. && cross(*a, *b, *d) == 0.)
                || ((*a == *c || *a == *d) && p == *a)
                || ((*b == *c || *b == *d) && p == *b)
            {
                return Ok(e);
            }
            let a = PointBox::around(*a, e);
            let b = PointBox::around(*b, e);
            let c = PointBox::around(*c, e);
            let d = PointBox::around(*d, e);
            let v = b.sub(a);
            let w = d.sub(c);
            let t = c.sub(a).cross(w).div(v.cross(w))?;
            vec![a.add(v.scale(t))]
        }
        (RegionEdge::Line { start, end }, RegionEdge::Arc(arc))
        | (RegionEdge::Arc(arc), RegionEdge::Line { start, end }) => {
            let start = PointBox::around(*start, e);
            let end = PointBox::around(*end, e);
            let c = PointBox::around(arc.center, e);
            let v = end.sub(start);
            let u = v.divide(v.norm()?)?;
            let z = c.sub(start);
            let along = z.dot(u);
            let perpendicular = z.cross(u);
            let r = Interval::around(arc.radius(), e);
            let h = r.square().sub(perpendicular.square()).sqrt()?;
            vec![
                start.add(u.scale(along.sub(h))),
                start.add(u.scale(along.add(h))),
            ]
        }
        (RegionEdge::Arc(a), RegionEdge::Arc(b)) => {
            let c = PointBox::around(a.center, e);
            let d = PointBox::around(b.center, e);
            let v = d.sub(c);
            let distance2 = v.x.square().add(v.y.square());
            let distance = distance2.sqrt()?;
            let r = Interval::around(a.radius(), e);
            let s = Interval::around(b.radius(), e);
            // Factored Heron height; no silent near-tangency clamp.
            let h2 = r
                .add(s)
                .square()
                .sub(distance2)
                .mul(distance2.sub(r.sub(s).square()))
                .div(distance2.mul(Interval::exact(4.)))?;
            let h = h2.sqrt()?;
            let along = r
                .square()
                .sub(s.square())
                .add(distance2)
                .div(distance.mul(Interval::exact(2.)))?;
            let u = v.divide(distance)?;
            let base = c.add(u.scale(along));
            let perpendicular = PointBox {
                x: u.y.neg(),
                y: u.x,
            }
            .scale(h);
            vec![base.add(perpendicular), base.sub(perpendicular)]
        }
    };
    let mut matches = boxes.into_iter().filter(|b| b.contains(p));
    let enclosure = matches
        .next()
        .ok_or_else(|| uncertain("nominal cut outside certified intersection enclosure"))?;
    if matches.next().is_some() {
        return Err(uncertain("intersection branches not numerically separable"));
    }
    let error = enclosure.radius_from(p);
    if !error.is_finite() {
        return Err(QueryError::NumericOverflow);
    }
    Ok(error)
}
