//! GF(2^8) with primitive polynomial `x^8 + x^4 + x^3 + x^2 + 1` (`0x11D`).

use std::sync::OnceLock;

/// Shared exp/log tables for multiply/divide.
static TABLES: OnceLock<GfTables> = OnceLock::new();

struct GfTables {
    exp: [u8; 512],
    log: [u8; 256],
}

fn tables() -> &'static GfTables {
    TABLES.get_or_init(|| {
        let mut exp = [0u8; 512];
        let mut log = [0u8; 256];
        log[0] = 0;
        let mut x = 1u8;
        for i in 0..255 {
            exp[i] = x;
            log[x as usize] = u8::try_from(i).expect("i < 255");
            x = mul_by_generator(x);
        }
        for i in 255..512 {
            exp[i] = exp[i - 255];
        }
        GfTables { exp, log }
    })
}

fn mul_by_generator(x: u8) -> u8 {
    if x & 0x80 != 0 {
        (x << 1) ^ 0x1D
    } else {
        x << 1
    }
}

/// Field addition (= XOR).
#[must_use]
pub(super) fn add(a: u8, b: u8) -> u8 {
    a ^ b
}

/// Field multiplication.
#[must_use]
pub(super) fn mul(a: u8, b: u8) -> u8 {
    if a == 0 || b == 0 {
        return 0;
    }
    let t = tables();
    let sum = u16::from(t.log[a as usize]) + u16::from(t.log[b as usize]);
    t.exp[(sum % 255) as usize]
}

/// Multiplicative inverse; `0` maps to `0` (callers must avoid dividing by zero).
#[must_use]
pub(super) fn inv(a: u8) -> u8 {
    if a == 0 {
        return 0;
    }
    let t = tables();
    t.exp[(255 - u16::from(t.log[a as usize])) as usize]
}

/// Raise `a` to the power `n` in GF(256).
#[must_use]
pub(super) fn pow(a: u8, mut n: u32) -> u8 {
    let mut acc = 1u8;
    let mut base = a;
    while n > 0 {
        if n & 1 != 0 {
            acc = mul(acc, base);
        }
        base = mul(base, base);
        n >>= 1;
    }
    acc
}

/// `out[i] = mul(c, input[i])` for every byte (Jerasure/Backblaze-style slice op).
pub(super) fn mul_slice(c: u8, input: &[u8], out: &mut [u8]) {
    assert_eq!(input.len(), out.len());
    if c == 0 {
        out.fill(0);
        return;
    }
    for (dst, &src) in out.iter_mut().zip(input) {
        *dst = mul(c, src);
    }
}

/// `out[i] ^= mul(c, input[i])` for every byte.
pub(super) fn mul_slice_add(c: u8, input: &[u8], out: &mut [u8]) {
    assert_eq!(input.len(), out.len());
    if c == 0 {
        return;
    }
    if c == 1 {
        for (dst, &src) in out.iter_mut().zip(input) {
            *dst ^= src;
        }
        return;
    }
    for (dst, &src) in out.iter_mut().zip(input) {
        *dst = add(*dst, mul(c, src));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_inverse_roundtrip() {
        for a in 1u8..=255 {
            assert_eq!(mul(a, inv(a)), 1, "a={a}");
        }
    }

    #[test]
    fn pow_matches_repeated_mul() {
        let a = 3u8;
        let mut p = 1u8;
        for n in 0..20 {
            assert_eq!(pow(a, n), p);
            p = mul(p, a);
        }
    }

    #[test]
    fn mul_slice_matches_scalar() {
        let input = [1u8, 2, 3, 4];
        let mut out = [0u8; 4];
        mul_slice(5, &input, &mut out);
        assert_eq!(out, [5, 10, 15, 20]);
    }

    #[test]
    fn mul_slice_add_matches_scalar() {
        let input = [1u8, 2, 3, 4];
        let mut out = [10u8, 20, 30, 40];
        let mut expect = out;
        for (e, s) in expect.iter_mut().zip(input) {
            *e = add(*e, mul(5, s));
        }
        mul_slice_add(5, &input, &mut out);
        assert_eq!(out, expect);
    }
}
