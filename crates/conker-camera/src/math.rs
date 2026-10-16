//! The game's own float helpers, ported instruction for instruction from the recompiled code so the
//! camera (and the camera-relative stick that depends on it) is bit-identical on every target.
//! Single-precision IEEE ops in the same order as the MIPS code; Rust never fuses them.

/// `sinf` (0x15047D60, libultra): Cody-Waite reduction by pi, degree-9 odd polynomial.
pub fn sin(x: f32) -> f32 {
    let e = (x.to_bits() as i32 >> 22) & 0x1FF;
    if e < 0xFF {
        if e < 0xE6 {
            return x;
        }
        let x2 = x * x;
        return x * x2 * poly(x2) + x;
    }
    if e < 0x136 {
        let q = x * INV_PI;
        let n = if 0.0 <= q {
            (q + 0.5) as i32
        } else {
            (q - 0.5) as i32
        };
        let r = reduce(x, n as f32);
        let r2 = r * r;
        let s = r * r2 * poly(r2) + r;
        return if n & 1 != 0 { -s } else { s };
    }
    if x.is_nan() { f32::NAN } else { 0.0 }
}

/// `cosf` (0x15047C00, libultra): cos x = sin(|x| - (n - 1/2) pi).
pub fn cos(x: f32) -> f32 {
    let e = (x.to_bits() as i32 >> 22) & 0x1FF;
    if e >= 0x136 {
        return if x.is_nan() { f32::NAN } else { 0.0 };
    }
    let ax = if 0.0 < x { x } else { -x };
    let q = ax * INV_PI + 0.5;
    let n = if 0.0 <= q {
        (q + 0.5) as i32
    } else {
        (q - 0.5) as i32
    };
    let r = reduce(ax, n as f32 - 0.5);
    let r2 = r * r;
    let c = r * r2 * poly(r2) + r;
    if n & 1 != 0 { -c } else { c }
}

const INV_PI: f32 = f32::from_bits(0x3EA2_F984);
const PI_HI: f32 = f32::from_bits(0x4049_0FDC);
const PI_LO: f32 = f32::from_bits(0x3308_85A3);
/// 0x80098D70: 1, -1/3!, 1/5!, ... (the leading 1 is unused).
const SIN: [f32; 4] = [
    f32::from_bits(0xBE2A_AAA6),
    f32::from_bits(0x3C08_8769),
    f32::from_bits(0xB94F_B7FD),
    f32::from_bits(0x362E_DEFA),
];

fn reduce(x: f32, n: f32) -> f32 {
    x - n * PI_HI - n * PI_LO
}

fn poly(x2: f32) -> f32 {
    SIN[0] + ((SIN[3] * x2 + SIN[2]) * x2 + SIN[1]) * x2
}

/// `func_15048C30`: the game's `asinf` (rational approximations on three ranges).
pub fn asin(x: f32) -> f32 {
    const P: [f32; 5] = [
        f32::from_bits(0x4067_A6B9),
        f32::from_bits(0xC0CE_F084),
        f32::from_bits(0x4060_AF71),
        f32::from_bits(0xBF10_7BFE),
        f32::from_bits(0x3BC0_69E8),
    ];
    const Q: [f32; 4] = [
        f32::from_bits(0x41AD_BD0A),
        f32::from_bits(0xC242_4AA8),
        f32::from_bits(0x4214_6B71),
        f32::from_bits(0xC130_8274),
    ];
    const R: [f32; 3] = [
        f32::from_bits(0xC04B_D00F),
        f32::from_bits(0x404D_7582),
        f32::from_bits(0xBF1B_93CD),
    ];
    const S: [f32; 3] = [
        f32::from_bits(0xC198_DC0D),
        f32::from_bits(0x41DE_E18C),
        f32::from_bits(0xC131_0B9F),
    ];
    const PI_4: f32 = f32::from_bits(0x3F49_0FD8);
    const PI_2: f32 = f32::from_bits(0x3FC9_0FD8);
    // p(t) / q(t) for t = x^2 (or the reduced argument's square).
    let pq = |t: f32| {
        let p = P[0] + (((P[4] * t + P[3]) * t + P[2]) * t + P[1]) * t;
        let q = Q[0] + (((t + Q[3]) * t + Q[2]) * t + Q[1]) * t;
        (p, q)
    };
    let bits = x.to_bits() as i32;
    let e = (bits >> 23) & 0xFF;
    if e < 0x7E {
        if e < 0x63 {
            return x;
        }
        let x2 = x * x;
        let (p, q) = pq(x2);
        return x2 * p * x / q + x;
    }
    if e < 0x7F {
        let ax = x.abs();
        if ax < f32::from_bits(0x3F5D_B3D0) {
            // asin x = pi/4 + asin(2x^2 - 1) / 2
            let t = (x * x + x * x) - 1.0;
            let t2 = t * t;
            let (p, q) = pq(t2);
            let h = 0.5 * (t2 * p * t / q + t);
            return if bits > 0 { h + PI_4 } else { -PI_4 - h };
        }
        // asin x = pi/2 - 2 asin(sqrt((1 - x) / 2))
        let z = (1.0 - ax) * 0.5;
        let s = z.sqrt();
        let r = R[0] + (R[2] * z + R[1]) * z;
        let q = S[0] + ((z + S[2]) * z + S[1]) * z;
        let a = z * r * s / q + s;
        return if bits > 0 {
            PI_2 - (a + a)
        } else {
            (a + a) + -PI_2
        };
    }
    if x.is_nan() {
        0.0
    } else if x == 1.0 {
        PI_2
    } else if x == -1.0 {
        -PI_2
    } else {
        0.0
    }
}

/// `func_150AD78C`: the level code's own `sinf` (hand-written: one shared polynomial, the reduction
/// rounds to nearest), a few ulps from [`sin`]. The camera volumes' sines come from it.
pub fn sin_level(x: f32) -> f32 {
    const C: [f32; 4] = [
        f32::from_bits(0x362E_DEF8),
        f32::from_bits(0xB94F_B7FF),
        f32::from_bits(0x3C08_876A),
        f32::from_bits(0xBE2A_AAA6),
    ];
    let poly = |x2: f32| ((C[0] * x2 + C[1]) * x2 + C[2]) * x2 + C[3];
    let e = (x.to_bits() as i32 >> 22) & 0x1FF;
    if e < 0xFF {
        if e < 0xE6 {
            return x;
        }
        let x2 = x * x;
        return poly(x2) * (x2 * x) + x;
    }
    if e >= 0x136 {
        return 0.0;
    }
    let n = (f32::from_bits(0x3EA2_F983) * x).round_ties_even() as i32; // cvt.w.s
    let nf = n as f32;
    let r = x - PI_HI_LEVEL * nf - PI_LO * nf;
    let r2 = r * r;
    let s = poly(r2) * r2 * r + r;
    if n & 1 != 0 { -s } else { s }
}

/// `func_150AD780`: [`sin_level`] of `x + pi/2`.
pub fn cos_level(x: f32) -> f32 {
    sin_level(x + f32::from_bits(0x3FC9_0FDB))
}

const PI_HI_LEVEL: f32 = f32::from_bits(0x4049_0FDB);

pub const DEG_TO_RAD: f32 = f32::from_bits(0x3C8E_FA35);
const RAD_TO_DEG: f32 = f32::from_bits(0x4265_2EE0);

/// `func_15048FC8`: yaw in degrees [0, 360) of the XZ vector `(x, z)`, such that a camera at yaw `a`
/// sits at `look_at + d (sin a, cos a)` and looks along `-(sin a, cos a)`.
pub fn yaw_of(x: f32, z: f32) -> f32 {
    let len = (x * x + z * z).sqrt();
    if len == 0.0 {
        return 0.0;
    }
    let a = asin(-x / len);
    let t = (if 0.0 < z {
        270.0 - a * RAD_TO_DEG
    } else {
        a * RAD_TO_DEG + 90.0
    }) - 90.0;
    if t < 0.0 { t + 360.0 } else { t }
}

/// `func_15048A70`: `b - a` wrapped to (-180, 180] for angles in [0, 360).
pub fn angle_diff(mut a: f32, mut b: f32) -> f32 {
    let d = a - b;
    if 180.0 < d {
        a -= 360.0;
    } else if d <= -180.0 {
        b -= 360.0;
    }
    b - a
}

/// `func_15048758`: wraps into [0, 360).
pub fn wrap360(mut a: f32) -> f32 {
    while a < 0.0 {
        a += 360.0;
    }
    while 360.0 <= a {
        a -= 360.0;
    }
    a
}

/// A damped spring toward `target`, one tick (`func_150495B0`): the velocity relaxes toward
/// `k * error` at rate `damp`, and the value stops exactly on the target instead of crossing it.
pub fn spring(x: &mut f32, target: f32, v: &mut f32, k: f32, damp: f32, dt: f32) {
    let sign = if target - *x < 0.0 { -1 } else { 1 };
    *v += (((target - *x) * k - *v) * damp) * dt;
    let next = *v * dt + *x;
    if (if target < next { -1 } else { 1 }) == sign {
        *x = next;
    } else {
        *x = target;
        *v = 0.0;
    }
}

/// The same spring on an angle in degrees (`func_15049688`): shortest way round, snapping onto the
/// target only within 10 degrees of it, result wrapped to [0, 360).
pub fn spring_angle(a: &mut f32, target: f32, v: &mut f32, k: f32, damp: f32, dt: f32) {
    let d = angle_diff(*a, target);
    let sign = if d < 0.0 { -1 } else { 1 };
    *v += ((d * k - *v) * damp) * dt;
    if d.abs() < 10.0
        && (if angle_diff(*v * dt + *a, target) < 0.0 {
            -1
        } else {
            1
        }) != sign
    {
        *a = target;
        *v = 0.0;
    } else {
        *a += *v * dt;
    }
    *a = wrap360(*a);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trig_matches_libm_closely() {
        // libultra's pi split is 3.6e-7 too big, so the error grows with the number of half turns.
        for i in -700..700 {
            let x = i as f32 * 0.01;
            assert!((sin(x) as f64 - (x as f64).sin()).abs() < 1e-6, "sin {x}");
            assert!((cos(x) as f64 - (x as f64).cos()).abs() < 1e-6, "cos {x}");
        }
        for i in -1000..=1000 {
            let x = i as f32 * 0.001;
            assert!(
                (asin(x) as f64 - (x as f64).asin()).abs() < 6e-7,
                "asin {x}"
            );
        }
        assert_eq!(asin(1.0), f32::from_bits(0x3FC9_0FD8));
        assert_eq!(sin(1e-5), 1e-5);
    }

    #[test]
    fn yaw_convention() {
        // A camera 247 behind a player facing u16 16896 (92.8125 deg) sits at yaw 2.8125.
        let a = 2.8125 * DEG_TO_RAD;
        let (dx, dz) = (sin(a) * 247.0, cos(a) * 247.0);
        assert!((yaw_of(-dx, -dz) - 2.8125).abs() < 1e-4);
        assert_eq!(yaw_of(0.0, -1.0), 0.0);
        // asin(1) is the game's 1.570796, a hair under pi/2.
        for (x, z, yaw) in [(-1.0, 0.0, 90.0), (0.0, 1.0, 180.0), (1.0, 0.0, 270.0)] {
            assert!((yaw_of(x, z) - yaw).abs() < 1e-4);
        }
    }

    #[test]
    fn angles_wrap() {
        assert_eq!(angle_diff(350.0, 10.0), 20.0);
        assert_eq!(angle_diff(10.0, 350.0), -20.0);
        assert_eq!(wrap360(-30.0), 330.0);
        assert_eq!(wrap360(725.0), 5.0);
    }

    #[test]
    fn spring_settles_without_overshoot() {
        let (mut x, mut v) = (0.0, 0.0);
        let dt = 1.0 / 30.0;
        // Matches the oracle's distance spring: 247 -> 247.387 run-up steps are covered by the
        // trace tests; here only the shape: monotone, then exactly on target.
        for _ in 0..200 {
            let prev = x;
            spring(&mut x, 100.0, &mut v, 6.0, 8.0, dt);
            assert!(x >= prev && x <= 100.0);
        }
        assert_eq!((x, v), (100.0, 0.0));
        // Angle spring takes the short way across 0 and snaps on arrival.
        let (mut a, mut w) = (350.0, 0.0);
        for _ in 0..200 {
            spring_angle(&mut a, 20.0, &mut w, 1.0, 2.8, dt);
            assert!(!(20.0..350.0).contains(&a) || a == 20.0);
        }
        assert_eq!((a, w), (20.0, 0.0));
    }
}
