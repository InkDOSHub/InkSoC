// bcrypt's Unix side for asymmetric keys: what Wine does with GnuTLS, done here.
//
// Wine's bcrypt.dll keeps the key object (struct key) and asks its Unix half to import,
// export, duplicate and destroy the key material, and to verify signatures. The engine
// keeps the material in a table; the key's private[0] holds its id there.
//
// Covered: RSA and ECC (P-256, P-384, P-521) public and private blobs; signature checks
// for RSA (PKCS #1 v1.5, PSS with SHA-1/SHA-256) and ECDSA. Not covered (they fail the way
// Wine fails without GnuTLS): symmetric ciphers, key generation, signing, encryption,
// DSA and DH, and secret agreement.

use crate::status::*;
use crate::Engine;
use std::collections::HashMap;

const NOT_SUPPORTED: u64 = 0xC000_00BB;
const INVALID_SIGNATURE: u64 = 0xC000_A000;
const INVALID_HANDLE: u64 = 0xC000_0008;
const BUFFER_TOO_SMALL: u64 = 0xC000_0023;
const INVALID_PARAMETER: u64 = 0xC000_000D;

// enum alg_id in Wine's bcrypt_internal.h
const ALG_RSA: u32 = 10;
const ALG_ECDH_FIRST: u32 = 12;
const ALG_ECDH_LAST: u32 = 15;
const ALG_RSA_SIGN: u32 = 16;
const ALG_ECDSA_FIRST: u32 = 17;
const ALG_ECDSA_LAST: u32 = 20;

const KEY_FLAG_FINALIZED: u32 = 2;
const EXPORT_PUBLIC: u32 = 1;
const IMPORT_PUBLIC: u32 = 1;
const PAD_PKCS1: u32 = 2;
const PAD_PSS: u32 = 8;

const RSA_PUBLIC: u32 = 0x3141_5352;
const RSA_PRIVATE: u32 = 0x3241_5352;
const RSA_FULLPRIVATE: u32 = 0x3341_5352;

#[derive(Clone)]
enum Material {
    /// public exponent and modulus (big-endian), and the blob it came from when private
    Rsa { e: Vec<u8>, n: Vec<u8>, private: Option<Vec<u8>> },
    /// coordinate size, public magic, X and Y, and the private blob
    Ecc { size: usize, magic: u32, x: Vec<u8>, y: Vec<u8>, private: Option<Vec<u8>> },
}

static mut KEYS: Option<HashMap<u64, Material>> = None;
static mut NEXT: u64 = 1;

#[allow(static_mut_refs)]
fn keys() -> &'static mut HashMap<u64, Material> {
    unsafe { KEYS.get_or_insert_with(HashMap::new) }
}

/// One unix call of bcrypt (`code`: enum key_funcs). Under WoW64 the parameters are the
/// 32-bit layouts of Wine's wow64 thunks.
pub fn call(e: &mut Engine, code: u32, params: u64) -> u64 {
    let w = e.s.wow64;
    let p = |i: u64| if w { 4 * i } else { 8 * i };
    match code {
        0 | 1 => OK, // process_attach, process_detach
        11 => {
            // duplicate (orig, copy)
            let (a, b) = (e.rptr(params), e.rptr(params + p(1)));
            let id = e.r64(a + 8);
            let Some(m) = keys().get(&id).cloned() else { return INVALID_HANDLE };
            let n = new_id();
            keys().insert(n, m);
            e.w64(b + 8, n);
            OK
        }
        13 => {
            // verify (key, padding, hash, hash_len, signature, signature_len, flags)
            let key = e.rptr(params);
            let pad = e.rptr(params + p(1));
            let hash = e.rptr(params + p(2));
            let hash_len = e.r32(params + p(3));
            let sig = e.rptr(params + if w { 16 } else { 32 });
            let sig_len = e.r32(params + if w { 20 } else { 40 });
            let flags = e.r32(params + if w { 24 } else { 44 });
            let mut h = vec![0u8; hash_len.min(1024) as usize];
            let mut s = vec![0u8; sig_len.min(2048) as usize];
            let _ = e.mmu.read_bytes(hash, &mut h);
            let _ = e.mmu.read_bytes(sig, &mut s);
            let alg = e.r32(key + 4);
            let Some(m) = keys().get(&e.r64(key + 8)).cloned() else { return INVALID_HANDLE };
            match m {
                Material::Ecc { size, x, y, .. } if (ALG_ECDSA_FIRST..=ALG_ECDSA_LAST).contains(&alg) => {
                    let Some(c) = Curve::of_size(size) else { return NOT_SUPPORTED };
                    if s.len() != 2 * size {
                        return INVALID_SIGNATURE;
                    }
                    if c.verify(&h, &x, &y, &s[..size], &s[size..]) { OK } else { INVALID_SIGNATURE }
                }
                Material::Rsa { e: ex, n, .. } if alg == ALG_RSA || alg == ALG_RSA_SIGN => {
                    if pad == 0 {
                        return INVALID_PARAMETER;
                    }
                    let alg_id = e.rptr(pad);
                    let name = if alg_id == 0 { String::new() } else { e.read_wstr(alg_id, 16).to_ascii_uppercase() };
                    if flags & PAD_PKCS1 != 0 {
                        rsa_pkcs1_verify(&n, &ex, &name, &h, &s)
                    } else if flags & PAD_PSS != 0 {
                        let salt = e.r32(pad + p(1)) as usize;
                        rsa_pss_verify(&n, &ex, &name, salt, &h, &s)
                    } else {
                        INVALID_PARAMETER
                    }
                }
                _ => NOT_SUPPORTED,
            }
        }
        14 => {
            // destroy (key)
            let id = e.r64(params + 8);
            keys().remove(&id);
            OK
        }
        15 => export(e, params, w),
        16 => import(e, params, w),
        _ => NOT_IMPLEMENTED,
    }
}

fn new_id() -> u64 {
    unsafe {
        NEXT += 1;
        NEXT
    }
}

fn le32(b: &[u8], o: usize) -> usize {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize
}

fn strip(v: &[u8]) -> Vec<u8> {
    let i = v.iter().position(|&b| b != 0).unwrap_or(v.len().saturating_sub(1));
    v[i..].to_vec()
}

fn import(e: &mut Engine, params: u64, w: bool) -> u64 {
    let key = e.rptr(params);
    let flags = e.r32(params + if w { 4 } else { 8 });
    let buf = e.rptr(params + if w { 8 } else { 16 });
    let len = e.r32(params + if w { 12 } else { 24 }) as usize;
    let mut b = vec![0u8; len.min(1 << 16)];
    let _ = e.mmu.read_bytes(buf, &mut b);
    let alg = e.r32(key + 4);
    let m = if alg == ALG_RSA || alg == ALG_RSA_SIGN {
        // BCRYPT_RSAKEY_BLOB: Magic, BitLength, cbPublicExp, cbModulus, cbPrime1, cbPrime2
        if b.len() < 24 {
            return INVALID_PARAMETER;
        }
        let (ce, cn) = (le32(&b, 8), le32(&b, 12));
        if b.len() < 24 + ce + cn {
            return INVALID_PARAMETER;
        }
        let magic = le32(&b, 0) as u32;
        let private = (flags & IMPORT_PUBLIC == 0 && magic != RSA_PUBLIC).then(|| b.clone());
        Material::Rsa { e: strip(&b[24..24 + ce]), n: strip(&b[24 + ce..24 + ce + cn]), private }
    } else if (ALG_ECDH_FIRST..=ALG_ECDH_LAST).contains(&alg) || (ALG_ECDSA_FIRST..=ALG_ECDSA_LAST).contains(&alg) {
        // BCRYPT_ECCKEY_BLOB: dwMagic, cbKey, then X, Y (and d)
        if b.len() < 8 {
            return INVALID_PARAMETER;
        }
        let (magic, size) = (le32(&b, 0) as u32, le32(&b, 4));
        if b.len() < 8 + 2 * size {
            return INVALID_PARAMETER;
        }
        let public = flags & IMPORT_PUBLIC != 0;
        // public magics end in an odd digit ('1', '3', '5'), private ones in the next one
        let pub_magic = if public { magic } else { magic - 0x0100_0000 };
        Material::Ecc { size, magic: pub_magic, x: b[8..8 + size].to_vec(), y: b[8 + size..8 + 2 * size].to_vec(), private: (!public).then(|| b.clone()) }
    } else {
        return NOT_IMPLEMENTED;
    };
    let old = e.r64(key + 8);
    keys().remove(&old);
    let id = new_id();
    keys().insert(id, m);
    e.w64(key + 8, id);
    let f = e.r32(key + 40);
    e.w32(key + 40, f | KEY_FLAG_FINALIZED);
    OK
}

fn export(e: &mut Engine, params: u64, w: bool) -> u64 {
    let key = e.rptr(params);
    let flags = e.r32(params + if w { 4 } else { 8 });
    let buf = e.rptr(params + if w { 8 } else { 16 });
    let len = e.r32(params + if w { 12 } else { 24 }) as usize;
    let ret_len = e.rptr(params + if w { 16 } else { 32 });
    if e.r32(key + 40) & KEY_FLAG_FINALIZED == 0 {
        return INVALID_HANDLE;
    }
    let bitlen = e.r32(key + 32);
    let Some(m) = keys().get(&e.r64(key + 8)).cloned() else { return INVALID_HANDLE };
    let public = flags & EXPORT_PUBLIC != 0;
    let out = match m {
        Material::Rsa { e: ex, n, private } => {
            if public {
                let size = (bitlen as usize).div_ceil(8).max(n.len());
                let mut nn = vec![0u8; size - n.len()];
                nn.extend_from_slice(&n);
                let mut o = Vec::new();
                for v in [RSA_PUBLIC, bitlen, ex.len() as u32, nn.len() as u32, 0, 0] {
                    o.extend_from_slice(&v.to_le_bytes());
                }
                o.extend_from_slice(&ex);
                o.extend_from_slice(&nn);
                o
            } else {
                match private {
                    // a full private blob was asked for (flag 2) and only a private one is here: not made up
                    Some(p) if le32(&p, 0) as u32 == RSA_FULLPRIVATE || flags & 2 == 0 && le32(&p, 0) as u32 == RSA_PRIVATE => p,
                    _ => return NOT_SUPPORTED,
                }
            }
        }
        Material::Ecc { size, magic, x, y, private } => {
            if public {
                let mut o = Vec::new();
                o.extend_from_slice(&magic.to_le_bytes());
                o.extend_from_slice(&(size as u32).to_le_bytes());
                o.extend_from_slice(&x);
                o.extend_from_slice(&y);
                o
            } else {
                match private {
                    Some(p) => p,
                    None => return NOT_SUPPORTED,
                }
            }
        }
    };
    if ret_len != 0 {
        e.w32(ret_len, out.len() as u32);
    }
    if buf == 0 {
        return OK;
    }
    if len < out.len() {
        return BUFFER_TOO_SMALL;
    }
    let _ = e.mmu.write_bytes(buf, &out);
    OK
}

// ---- RSA ----

fn digest_info(name: &str) -> Option<(&'static [u8], usize)> {
    Some(match name {
        "MD5" => (&[0x30, 0x20, 0x30, 0x0c, 0x06, 0x08, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x02, 0x05, 0x05, 0x00, 0x04, 0x10], 16),
        "SHA1" => (&[0x30, 0x21, 0x30, 0x09, 0x06, 0x05, 0x2b, 0x0e, 0x03, 0x02, 0x1a, 0x05, 0x00, 0x04, 0x14], 20),
        "SHA256" => (&[0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05, 0x00, 0x04, 0x20], 32),
        "SHA384" => (&[0x30, 0x41, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x02, 0x05, 0x00, 0x04, 0x30], 48),
        "SHA512" => (&[0x30, 0x51, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x03, 0x05, 0x00, 0x04, 0x40], 64),
        _ => return None,
    })
}

/// s^e mod n as a big-endian block of n's length, or None when s >= n.
fn rsa_open(n: &[u8], ex: &[u8], sig: &[u8]) -> Option<Vec<u8>> {
    let (nn, s) = (Big::from_be(n), Big::from_be(sig));
    if nn.is_zero() || s.cmp(&nn) != std::cmp::Ordering::Less {
        return None;
    }
    Some(s.powmod(&Big::from_be(ex), &nn).to_be(n.len()))
}

pub(crate) fn rsa_pkcs1_verify(n: &[u8], ex: &[u8], name: &str, hash: &[u8], sig: &[u8]) -> u64 {
    let Some(em) = rsa_open(n, ex, sig) else { return INVALID_SIGNATURE };
    let mut t = Vec::new();
    if !name.is_empty() {
        let Some((prefix, hl)) = digest_info(name) else { return NOT_SUPPORTED };
        if hash.len() != hl {
            return INVALID_SIGNATURE;
        }
        t.extend_from_slice(prefix);
    }
    t.extend_from_slice(hash);
    if em.len() < t.len() + 11 {
        return INVALID_SIGNATURE;
    }
    let ps = em.len() - t.len() - 3;
    let ok = em[0] == 0 && em[1] == 1 && em[2..2 + ps].iter().all(|&b| b == 0xff) && em[2 + ps] == 0 && em[3 + ps..] == t[..];
    if ok { OK } else { INVALID_SIGNATURE }
}

fn hash_by_name(name: &str, d: &[u8]) -> Option<Vec<u8>> {
    match name {
        "SHA1" => Some(crate::hash::sha1(d)),
        "SHA256" => Some(crate::hash::sha256(d)),
        "MD5" => Some(crate::hash::md5(d)),
        _ => None,
    }
}

pub(crate) fn rsa_pss_verify(n: &[u8], ex: &[u8], name: &str, salt: usize, hash: &[u8], sig: &[u8]) -> u64 {
    let Some(hl) = hash_by_name(name, b"").map(|v| v.len()) else { return NOT_SUPPORTED };
    if hash.len() != hl {
        return INVALID_SIGNATURE;
    }
    let Some(full) = rsa_open(n, ex, sig) else { return INVALID_SIGNATURE };
    let mod_bits = Big::from_be(n).bits();
    let em_bits = mod_bits - 1;
    let em_len = em_bits.div_ceil(8);
    // the opened block has n's length; a leading zero byte when emLen is one less
    let lead = full.len() - em_len;
    if full[..lead].iter().any(|&b| b != 0) {
        return INVALID_SIGNATURE;
    }
    let em = &full[lead..];
    if em_len < hl + salt + 2 || em[em_len - 1] != 0xbc {
        return INVALID_SIGNATURE;
    }
    let (masked, h) = (&em[..em_len - hl - 1], &em[em_len - hl - 1..em_len - 1]);
    let top = 8 * em_len - em_bits;
    if top > 0 && masked[0] >> (8 - top) != 0 {
        return INVALID_SIGNATURE;
    }
    // MGF1
    let mut mask = Vec::new();
    let mut c = 0u32;
    while mask.len() < masked.len() {
        let mut d = h.to_vec();
        d.extend_from_slice(&c.to_be_bytes());
        mask.extend(hash_by_name(name, &d).unwrap());
        c += 1;
    }
    let mut db: Vec<u8> = masked.iter().zip(&mask).map(|(a, b)| a ^ b).collect();
    if top > 0 {
        db[0] &= 0xff >> top;
    }
    let ps = em_len - hl - salt - 2;
    if db[..ps].iter().any(|&b| b != 0) || db[ps] != 1 {
        return INVALID_SIGNATURE;
    }
    let mut m = vec![0u8; 8];
    m.extend_from_slice(hash);
    m.extend_from_slice(&db[ps + 1..]);
    if hash_by_name(name, &m).unwrap() == h { OK } else { INVALID_SIGNATURE }
}

// ---- ECDSA over the NIST prime curves (a = -3) ----

struct Curve {
    p: Big,
    n: Big,
    b: Big,
    g: (Big, Big),
}

fn hex(s: &str) -> Big {
    let v: Vec<u8> = (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect();
    Big::from_be(&v)
}

impl Curve {
    fn of_size(size: usize) -> Option<Curve> {
        Some(match size {
            32 => Curve {
                p: hex("ffffffff00000001000000000000000000000000ffffffffffffffffffffffff"),
                n: hex("ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551"),
                b: hex("5ac635d8aa3a93e7b3ebbd55769886bc651d06b0cc53b0f63bce3c3e27d2604b"),
                g: (
                    hex("6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296"),
                    hex("4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5"),
                ),
            },
            48 => Curve {
                p: hex("fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffeffffffff0000000000000000ffffffff"),
                n: hex("ffffffffffffffffffffffffffffffffffffffffffffffffc7634d81f4372ddf581a0db248b0a77aecec196accc52973"),
                b: hex("b3312fa7e23ee7e4988e056be3f82d19181d9c6efe8141120314088f5013875ac656398d8a2ed19d2a85c8edd3ec2aef"),
                g: (
                    hex("aa87ca22be8b05378eb1c71ef320ad746e1d3b628ba79b9859f741e082542a385502f25dbf55296c3a545e3872760ab7"),
                    hex("3617de4a96262c6f5d9e98bf9292dc29f8f41dbd289a147ce9da3113b5f0b8c00a60b1ce1d7e819d7a431d7c90ea0e5f"),
                ),
            },
            66 => Curve {
                p: hex("01ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"),
                n: hex("01fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffa51868783bf2f966b7fcc0148f709a5d03bb5c9b8899c47aebb6fb71e91386409"),
                b: hex("0051953eb9618e1c9a1f929a21a0b68540eea2da725b99b315f3b8b489918ef109e156193951ec7e937b1652c0bd3bb1bf073573df883d2c34f1ef451fd46b503f00"),
                g: (
                    hex("00c6858e06b70404e9cd9e3ecb662395b4429c648139053fb521f828af606b4d3dbaa14b5e77efe75928fe1dc127a2ffa8de3348b3c1856a429bf97e7e31c2e5bd66"),
                    hex("011839296a789a3bc0045c8a5fb42c7d1bd998f54449579b446817afbd17273e662c97ee72995ef42640c550b9013fad0761353c7086a272c24088be94769fd16650"),
                ),
            },
            _ => return None,
        })
    }

    fn on_curve(&self, x: &Big, y: &Big) -> bool {
        let p = &self.p;
        if x.cmp(p) != std::cmp::Ordering::Less || y.cmp(p) != std::cmp::Ordering::Less {
            return false;
        }
        // y^2 = x^3 - 3x + b
        let lhs = y.mulmod(y, p);
        let x3 = x.mulmod(x, p).mulmod(x, p);
        let three_x = x.add(x).add(x).rem(p);
        let rhs = x3.add(p).sub(&three_x).add(&self.b).rem(p);
        lhs == rhs
    }

    /// Jacobian doubling (a = -3); None is the point at infinity.
    fn double(&self, q: &Option<(Big, Big, Big)>) -> Option<(Big, Big, Big)> {
        let (x, y, z) = q.as_ref()?;
        if y.is_zero() {
            return None;
        }
        let p = &self.p;
        let z2 = z.mulmod(z, p);
        let m = x.add(p).sub(&z2).rem(p).mulmod(&x.add(&z2).rem(p), p).mulmod(&Big::from_u32(3), p);
        let y2 = y.mulmod(y, p);
        let s = x.mulmod(&y2, p).mulmod(&Big::from_u32(4), p);
        let x3 = m.mulmod(&m, p).add(p).add(p).sub(&s).sub(&s).rem(p);
        let y4 = y2.mulmod(&y2, p).mulmod(&Big::from_u32(8), p);
        let y3 = m.mulmod(&s.add(p).sub(&x3).rem(p), p).add(p).sub(&y4).rem(p);
        let z3 = y.mulmod(z, p).mulmod(&Big::from_u32(2), p);
        Some((x3, y3, z3))
    }

    fn add_pts(&self, a: &Option<(Big, Big, Big)>, b: &Option<(Big, Big, Big)>) -> Option<(Big, Big, Big)> {
        let Some((x1, y1, z1)) = a else { return b.clone() };
        let Some((x2, y2, z2)) = b else { return a.clone() };
        let p = &self.p;
        let z1s = z1.mulmod(z1, p);
        let z2s = z2.mulmod(z2, p);
        let u1 = x1.mulmod(&z2s, p);
        let u2 = x2.mulmod(&z1s, p);
        let s1 = y1.mulmod(&z2s.mulmod(z2, p), p);
        let s2 = y2.mulmod(&z1s.mulmod(z1, p), p);
        if u1 == u2 {
            return if s1 == s2 { self.double(a) } else { None };
        }
        let h = u2.add(p).sub(&u1).rem(p);
        let r = s2.add(p).sub(&s1).rem(p);
        let h2 = h.mulmod(&h, p);
        let h3 = h2.mulmod(&h, p);
        let u1h2 = u1.mulmod(&h2, p);
        let x3 = r.mulmod(&r, p).add(p).sub(&h3).add(p).sub(&u1h2).add(p).sub(&u1h2).rem(p);
        let y3 = r.mulmod(&u1h2.add(p).sub(&x3).rem(p), p).add(p).sub(&s1.mulmod(&h3, p)).rem(p);
        let z3 = h.mulmod(&z1.mulmod(z2, p), p);
        Some((x3, y3, z3))
    }

    /// u1*G + u2*Q (Shamir's trick)
    fn mul2(&self, u1: &Big, u2: &Big, q: &(Big, Big)) -> Option<(Big, Big, Big)> {
        let g = Some((self.g.0.clone(), self.g.1.clone(), Big::from_u32(1)));
        let q = Some((q.0.clone(), q.1.clone(), Big::from_u32(1)));
        let gq = self.add_pts(&g, &q);
        let mut r: Option<(Big, Big, Big)> = None;
        for i in (0..u1.bits().max(u2.bits())).rev() {
            r = self.double(&r);
            match (u1.bit(i), u2.bit(i)) {
                (true, true) => r = self.add_pts(&r, &gq),
                (true, false) => r = self.add_pts(&r, &g),
                (false, true) => r = self.add_pts(&r, &q),
                _ => {}
            }
        }
        r
    }

    fn verify(&self, hash: &[u8], qx: &[u8], qy: &[u8], r: &[u8], s: &[u8]) -> bool {
        let n = &self.n;
        let (qx, qy, r, s) = (Big::from_be(qx), Big::from_be(qy), Big::from_be(r), Big::from_be(s));
        if !self.on_curve(&qx, &qy) {
            return false;
        }
        let one_to_n = |v: &Big| !v.is_zero() && v.cmp(n) == std::cmp::Ordering::Less;
        if !one_to_n(&r) || !one_to_n(&s) {
            return false;
        }
        let mut e = Big::from_be(hash);
        let (hb, nb) = (8 * hash.len(), n.bits());
        if hb > nb {
            e = e.shr(hb - nb);
        }
        let e = e.rem(n);
        // s^-1 = s^(n-2) mod n (n is prime)
        let w = s.powmod(&n.sub(&Big::from_u32(2)), n);
        let (u1, u2) = (e.mulmod(&w, n), r.mulmod(&w, n));
        let Some((x, _, z)) = self.mul2(&u1, &u2, &(qx, qy)) else { return false };
        let p = &self.p;
        let zi = z.powmod(&p.sub(&Big::from_u32(2)), p);
        let ax = x.mulmod(&zi.mulmod(&zi, p), p);
        ax.rem(n) == r
    }
}

// ---- small unsigned big integers (little-endian 32-bit limbs) ----

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Big(Vec<u32>);

impl Big {
    fn norm(mut v: Vec<u32>) -> Big {
        while v.last() == Some(&0) {
            v.pop();
        }
        Big(v)
    }
    fn from_u32(x: u32) -> Big {
        Big::norm(vec![x])
    }
    pub(crate) fn from_be(b: &[u8]) -> Big {
        let mut v = Vec::with_capacity(b.len() / 4 + 1);
        for c in b.rchunks(4) {
            let mut w = 0u32;
            for &x in c {
                w = w << 8 | x as u32;
            }
            v.push(w);
        }
        Big::norm(v)
    }
    pub(crate) fn to_be(&self, len: usize) -> Vec<u8> {
        let mut out = vec![0u8; len];
        for i in 0..len {
            let (l, s) = (i / 4, 8 * (i % 4));
            let byte = self.0.get(l).map_or(0, |w| (w >> s) as u8);
            out[len - 1 - i] = byte;
        }
        out
    }
    fn is_zero(&self) -> bool {
        self.0.is_empty()
    }
    fn bits(&self) -> usize {
        match self.0.last() {
            Some(&w) => 32 * (self.0.len() - 1) + (32 - w.leading_zeros() as usize),
            None => 0,
        }
    }
    fn bit(&self, i: usize) -> bool {
        self.0.get(i / 32).is_some_and(|w| w >> (i % 32) & 1 != 0)
    }
    fn cmp(&self, o: &Big) -> std::cmp::Ordering {
        self.0.len().cmp(&o.0.len()).then_with(|| self.0.iter().rev().cmp(o.0.iter().rev()))
    }
    fn add(&self, o: &Big) -> Big {
        let mut v = Vec::with_capacity(self.0.len().max(o.0.len()) + 1);
        let mut c = 0u64;
        for i in 0..self.0.len().max(o.0.len()) {
            let s = *self.0.get(i).unwrap_or(&0) as u64 + *o.0.get(i).unwrap_or(&0) as u64 + c;
            v.push(s as u32);
            c = s >> 32;
        }
        v.push(c as u32);
        Big::norm(v)
    }
    /// self - o (self >= o)
    fn sub(&self, o: &Big) -> Big {
        let mut v = Vec::with_capacity(self.0.len());
        let mut br = 0i64;
        for i in 0..self.0.len() {
            let mut d = self.0[i] as i64 - *o.0.get(i).unwrap_or(&0) as i64 - br;
            br = 0;
            if d < 0 {
                d += 1 << 32;
                br = 1;
            }
            v.push(d as u32);
        }
        Big::norm(v)
    }
    fn mul(&self, o: &Big) -> Big {
        let mut v = vec![0u32; self.0.len() + o.0.len() + 1];
        for (i, &a) in self.0.iter().enumerate() {
            let mut c = 0u64;
            for (j, &b) in o.0.iter().enumerate() {
                let t = v[i + j] as u64 + a as u64 * b as u64 + c;
                v[i + j] = t as u32;
                c = t >> 32;
            }
            let mut k = i + o.0.len();
            while c != 0 {
                let t = v[k] as u64 + c;
                v[k] = t as u32;
                c = t >> 32;
                k += 1;
            }
        }
        Big::norm(v)
    }
    fn shl1_or(&mut self, bit: bool) {
        let mut c = bit as u32;
        for w in self.0.iter_mut() {
            let n = *w >> 31;
            *w = *w << 1 | c;
            c = n;
        }
        if c != 0 {
            self.0.push(c);
        }
    }
    fn shr(&self, k: usize) -> Big {
        let (ws, bs) = (k / 32, k % 32);
        let mut v = Vec::new();
        for i in ws..self.0.len() {
            let lo = self.0[i] >> bs;
            let hi = if bs > 0 { self.0.get(i + 1).map_or(0, |&w| w << (32 - bs)) } else { 0 };
            v.push(lo | hi);
        }
        Big::norm(v)
    }
    /// self mod m, bit by bit
    fn rem(&self, m: &Big) -> Big {
        if self.cmp(m) == std::cmp::Ordering::Less {
            return self.clone();
        }
        let mut r = Big(Vec::new());
        for i in (0..self.bits()).rev() {
            r.shl1_or(self.bit(i));
            if r.cmp(m) != std::cmp::Ordering::Less {
                r = r.sub(m);
            }
        }
        r
    }
    fn mulmod(&self, o: &Big, m: &Big) -> Big {
        self.mul(o).rem(m)
    }
    fn powmod(&self, e: &Big, m: &Big) -> Big {
        let mut r = Big::from_u32(1).rem(m);
        let b = self.rem(m);
        for i in (0..e.bits()).rev() {
            r = r.mulmod(&r, m);
            if e.bit(i) {
                r = r.mulmod(&b, m);
            }
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_are_consistent() {
        for size in [32, 48, 66] {
            let c = Curve::of_size(size).unwrap();
            assert!(c.on_curve(&c.g.0, &c.g.1), "G on curve {}", size);
            // n*G is the point at infinity: (n-1)*G + G
            let g = Some((c.g.0.clone(), c.g.1.clone(), Big::from_u32(1)));
            let nm1 = c.n.sub(&Big::from_u32(1));
            let mut r: Option<(Big, Big, Big)> = None;
            for i in (0..nm1.bits()).rev() {
                r = c.double(&r);
                if nm1.bit(i) {
                    r = c.add_pts(&r, &g);
                }
            }
            assert!(c.add_pts(&r, &g).is_none(), "n*G = O on curve {}", size);
        }
    }

    #[test]
    fn ecdsa_p256_known_answer() {
        // RFC 6979 A.2.5, P-256, SHA-256, message "sample"
        let c = Curve::of_size(32).unwrap();
        let ux = hex("60fed4ba255a9d31c961eb74c6356d68c049b8923b61fa6ce669622e60f29fb6").to_be(32);
        let uy = hex("7903fe1008b8bc99a41ae9e95628bc64f2f1b20c2d7e9f5177a3c294d4462299").to_be(32);
        let h = crate::hash::sha256(b"sample");
        let r = hex("efd48b2aacb6a8fd1140dd9cd45e81d69d2c877b56aaf991c34d0ea84eaf3716").to_be(32);
        let s = hex("f7cb1c942d657c41d436c7a1b6e29f65f3e900dbb9aff4064dc4ab2f843acda8").to_be(32);
        assert!(c.verify(&h, &ux, &uy, &r, &s));
        let mut bad = s.clone();
        bad[31] ^= 1;
        assert!(!c.verify(&h, &ux, &uy, &r, &bad));
    }

    #[test]
    fn rsa_small_pkcs1() {
        // a toy key: n = 61*53*... is too small for PKCS#1; check the arithmetic instead
        let n = Big::from_u32(3233);
        let m = Big::from_u32(65);
        let c = m.powmod(&Big::from_u32(17), &n);
        assert_eq!(c, Big::from_u32(2790));
        assert_eq!(c.powmod(&Big::from_u32(2753), &n), m);
    }
}
