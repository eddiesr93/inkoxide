//! Encryption with the standard security handler, revision 6 (ISO
//! 32000-2, 7.6.4): AES-256 for every string and stream, the `U`, `UE`,
//! `O`, `OE` and `Perms` entries (Algorithms 2.A, 2.B, 8, 9 and 10).
//!
//! Revision 6 asks for a random file key and salts, and the encryption of
//! each string and stream for a random IV. The engine has no randomness,
//! and identical input must give identical output, so everything is
//! derived instead, from a digest `D` of the document and the passwords:
//!
//! - the salts: `HMAC(D, "salts")`;
//! - the file key: `HMAC(IK_u ‖ IK_o, "key" ‖ D)`, where `IK_u` and
//!   `IK_o` are the intermediate keys of `UE` and `OE` (each an Algorithm
//!   2.B hash of a password), so a guess costs one such hash, as it does
//!   through `U` or `O`;
//! - each IV: `HMAC(K_iv, where ‖ plaintext)`, a synthetic IV, so one key
//!   and IV never encrypt two different plaintexts;
//! - the file identifier and the four free bytes of `Perms`: HMACs keyed
//!   by the file key.
//!
//! The derivation is not the standard's random one and needs a
//! cryptography review before release.

use aes::cipher::{BlockCipherEncrypt, KeyInit};
use aes::{Aes128, Aes256};
use pdf_writer::{Chunk, Name, Ref, Str};
use sha2::{Digest, Sha256, Sha384, Sha512};

use crate::schema::Meta;

/// Passwords and permissions of an encrypted file, checked: printable
/// ASCII, at most [`MAX_PASSWORD`] bytes each.
#[derive(Clone)]
pub(crate) struct Params {
    pub user: Vec<u8>,
    pub owner: Vec<u8>,
    /// `/P`: the permission bits.
    pub p: i32,
}

impl std::fmt::Debug for Params {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Params")
            .field("user", &"<redacted>")
            .field("owner", &"<redacted>")
            .field("p", &self.p)
            .finish()
    }
}

/// Longest password, in bytes (ISO 32000-2, 7.6.4.3.3).
pub(crate) const MAX_PASSWORD: usize = 127;

/// The entries of the `Encrypt` dictionary and the file identifier.
pub(crate) struct Dict {
    pub o: [u8; 48],
    pub u: [u8; 48],
    pub oe: [u8; 32],
    pub ue: [u8; 32],
    pub perms: [u8; 16],
    pub p: i32,
    pub id: [u8; 16],
}

impl Dict {
    /// Writes the `Encrypt` dictionary as object `r`. Its strings are not
    /// encrypted.
    pub fn write(&self, pdf: &mut Chunk, r: Ref) {
        let mut e = pdf.indirect(r).dict();
        e.pair(Name(b"Filter"), Name(b"Standard"))
            .pair(Name(b"V"), 5)
            .pair(Name(b"R"), 6)
            .pair(Name(b"Length"), 256);
        e.insert(Name(b"CF"))
            .dict()
            .insert(Name(b"StdCF"))
            .dict()
            .pair(Name(b"Type"), Name(b"CryptFilter"))
            .pair(Name(b"CFM"), Name(b"AESV3"))
            .pair(Name(b"AuthEvent"), Name(b"DocOpen"))
            .pair(Name(b"Length"), 32);
        e.pair(Name(b"StmF"), Name(b"StdCF"))
            .pair(Name(b"StrF"), Name(b"StdCF"))
            .pair(Name(b"O"), Str(&self.o))
            .pair(Name(b"U"), Str(&self.u))
            .pair(Name(b"OE"), Str(&self.oe))
            .pair(Name(b"UE"), Str(&self.ue))
            .pair(Name(b"P"), self.p)
            .pair(Name(b"Perms"), Str(&self.perms))
            .pair(Name(b"EncryptMetadata"), true);
    }
}

/// Adds `/Encrypt r` to the trailer of `pdf`, as pdf-writer finished it
/// (pdf-writer 0.15 writes no `Encrypt` entry). The trailer follows the
/// cross-reference table, so no offset moves. `None` if there is no
/// trailer where pdf-writer puts it.
pub(crate) fn splice_encrypt(mut pdf: Vec<u8>, r: Ref) -> Option<Vec<u8>> {
    const END: &[u8] = b"\n>>\nstartxref\n";
    let trailer = pdf.windows(9).rposition(|w| w == b"\ntrailer\n")?;
    let at = trailer + pdf[trailer..].windows(END.len()).position(|w| w == END)?;
    let entry = format!("\n  /Encrypt {} 0 R", r.get());
    pdf.splice(at..at, entry.into_bytes());
    Some(pdf)
}

/// Encrypts strings and streams with the file key.
pub(crate) struct Crypt {
    cipher: Aes256,
    /// HMAC key of the IVs.
    iv_key: [u8; 32],
    /// Strings encrypted so far: where the next one is.
    strings: u64,
}

impl Crypt {
    /// The keys of a file encrypted with `params` whose content digests to
    /// `d` ([`digest`]), and its `Encrypt` dictionary.
    pub fn new(params: &Params, d: &[u8; 32]) -> (Self, Dict) {
        let salts = hmac(d, &[b"salts"]);
        let (uvs, uks, ovs, oks) = (&salts[..8], &salts[8..16], &salts[16..24], &salts[24..]);
        let mut u = [0; 48];
        u[..32].copy_from_slice(&hash_2b(&params.user, uvs, &[]));
        u[32..40].copy_from_slice(uvs);
        u[40..].copy_from_slice(uks);
        let ik_u = hash_2b(&params.user, uks, &[]);
        let mut o = [0; 48];
        o[..32].copy_from_slice(&hash_2b(&params.owner, ovs, &u));
        o[32..40].copy_from_slice(ovs);
        o[40..].copy_from_slice(oks);
        let ik_o = hash_2b(&params.owner, oks, &u);
        let key = hmac(&[ik_u, ik_o].concat(), &[b"key", d]);

        let wrap = |ik: &[u8; 32]| {
            let mut out = [0; 32];
            out.copy_from_slice(&cbc_encrypt(&Aes256::new(&(*ik).into()), &[0; 16], &key));
            out
        };
        let cipher = Aes256::new(&key.into());
        let mut perms = [0; 16];
        perms[..4].copy_from_slice(&params.p.to_le_bytes());
        perms[4..8].fill(0xFF);
        // `EncryptMetadata` is true.
        perms[8..12].copy_from_slice(b"Tadb");
        perms[12..].copy_from_slice(&hmac(&key, &[b"perms"])[..4]);
        let mut block = aes::Block::from(perms);
        cipher.encrypt_block(&mut block);
        let mut id = [0; 16];
        id.copy_from_slice(&hmac(&key, &[b"id"])[..16]);
        let dict = Dict {
            o,
            u,
            oe: wrap(&ik_o),
            ue: wrap(&ik_u),
            perms: block.into(),
            p: params.p,
            id,
        };
        let crypt = Self {
            cipher,
            iv_key: hmac(&key, &[b"iv"]),
            strings: 0,
        };
        (crypt, dict)
    }

    /// String `plain`, encrypted: its IV, then the ciphertext.
    pub fn string(&mut self, plain: &[u8]) -> Vec<u8> {
        self.strings += 1;
        self.seal(&[b"string", &self.strings.to_be_bytes()], plain)
    }

    /// The data of stream object `r`, encrypted.
    pub fn stream(&self, r: Ref, plain: &[u8]) -> Vec<u8> {
        self.seal(&[b"stream", &r.get().to_be_bytes()], plain)
    }

    /// AES-256-CBC with PKCS#5 padding, the IV first (ISO 32000-2,
    /// 7.6.3); the IV is a MAC of `place` and `plain`.
    fn seal(&self, place: &[&[u8]], plain: &[u8]) -> Vec<u8> {
        let mac = hmac(&self.iv_key, &[place, &[plain]].concat());
        let iv: [u8; 16] = mac[..16].try_into().unwrap_or_default();
        let pad = 16 - plain.len() % 16;
        let mut data = Vec::with_capacity(plain.len() + pad);
        data.extend_from_slice(plain);
        data.resize(plain.len() + pad, pad as u8);
        let mut out = iv.to_vec();
        out.extend(cbc_encrypt(&self.cipher, &iv, &data));
        out
    }
}

/// What the keys are derived from: a SHA-256 of the page content streams
/// and the metadata. Neither depends on the input format or the thread
/// count.
pub(crate) fn digest<'p>(pages: impl IntoIterator<Item = &'p [u8]>, meta: &Meta) -> [u8; 32] {
    let mut h = Sha256::new();
    let field = |h: &mut Sha256, bytes: &[u8]| {
        h.update((bytes.len() as u64).to_be_bytes());
        h.update(bytes);
    };
    for page in pages {
        field(&mut h, page);
    }
    for v in [
        &meta.title,
        &meta.author,
        &meta.subject,
        &meta.keywords,
        &meta.creator,
        &meta.producer,
        &meta.lang,
        &meta.creation_date,
    ] {
        match v {
            Some(v) => field(&mut h, v.as_bytes()),
            None => h.update([0xFF]),
        }
    }
    h.finalize().into()
}

/// HMAC-SHA256 (RFC 2104) of the concatenation of `parts`.
fn hmac(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(k.map(|b| b ^ 0x36));
    for p in parts {
        inner.update(p);
    }
    let mut outer = Sha256::new();
    outer.update(k.map(|b| b ^ 0x5C));
    outer.update(inner.finalize());
    outer.finalize().into()
}

/// CBC encryption of `data`, a whole number of blocks, without padding.
fn cbc_encrypt<C: BlockCipherEncrypt<BlockSize = aes::cipher::consts::U16>>(
    cipher: &C,
    iv: &[u8; 16],
    data: &[u8],
) -> Vec<u8> {
    debug_assert!(data.len().is_multiple_of(16));
    let mut prev = *iv;
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks_exact(16) {
        let mut block = [0u8; 16];
        for (b, (x, p)) in block.iter_mut().zip(chunk.iter().zip(prev)) {
            *b = x ^ p;
        }
        let mut block = aes::Block::from(block);
        cipher.encrypt_block(&mut block);
        prev = block.into();
        out.extend_from_slice(&prev);
    }
    out
}

/// Algorithm 2.B (ISO 32000-2, 7.6.4.3.4): the hash of `password` with
/// `salt` and, for the owner password, the user key `udata`.
fn hash_2b(password: &[u8], salt: &[u8], udata: &[u8]) -> [u8; 32] {
    let mut k: Vec<u8> = Sha256::new()
        .chain_update(password)
        .chain_update(salt)
        .chain_update(udata)
        .finalize()
        .to_vec();
    let mut round = 0u32;
    loop {
        let mut k1 = Vec::with_capacity(64 * (password.len() + k.len() + udata.len()));
        for _ in 0..64 {
            k1.extend_from_slice(password);
            k1.extend_from_slice(&k);
            k1.extend_from_slice(udata);
        }
        let key: [u8; 16] = k[..16].try_into().unwrap_or_default();
        let iv: [u8; 16] = k[16..32].try_into().unwrap_or_default();
        let e = cbc_encrypt(&Aes128::new(&key.into()), &iv, &k1);
        // The first 16 bytes as a big-endian number, modulo 3: 256 is 1
        // modulo 3, so the sum of the bytes.
        let sum: u32 = e[..16].iter().map(|&b| u32::from(b)).sum();
        k = match sum % 3 {
            0 => Sha256::digest(&e).to_vec(),
            1 => Sha384::digest(&e).to_vec(),
            _ => Sha512::digest(&e).to_vec(),
        };
        round += 1;
        let last = u32::from(e[e.len() - 1]);
        if round >= 64 && last + 32 <= round {
            break;
        }
    }
    let mut out = [0; 32];
    out.copy_from_slice(&k[..32]);
    out
}

#[cfg(test)]
mod tests {
    use aes::cipher::BlockCipherDecrypt;

    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn hmac_matches_rfc_4231() {
        // Test cases 1, 2 and 6 (a key longer than a block).
        assert_eq!(
            hmac(&[0x0B; 20], &[b"Hi There"]).to_vec(),
            hex("b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7")
        );
        assert_eq!(
            hmac(b"Jefe", &[b"what do ya want ", b"for nothing?"]).to_vec(),
            hex("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843")
        );
        assert_eq!(
            hmac(
                &[0xAA; 131],
                &[b"Test Using Larger Than Block-Size Key - Hash Key First"]
            )
            .to_vec(),
            hex("60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54")
        );
    }

    #[test]
    fn cbc_matches_nist_sp_800_38a() {
        // F.2.5, CBC-AES256.Encrypt, the first two blocks.
        let key = hex("603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4");
        let iv: [u8; 16] = hex("000102030405060708090a0b0c0d0e0f").try_into().unwrap();
        let plain = hex("6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e51");
        let cipher = Aes256::new_from_slice(&key).unwrap();
        assert_eq!(
            cbc_encrypt(&cipher, &iv, &plain),
            hex("f58c4c04d6e5f1ba779eabfb5f7bfbd69cfc4e967edb808d679f777bc6702c7d")
        );
    }

    /// Decrypts AES-256 `block` with `key`.
    fn decrypt(key: &[u8], block: &[u8]) -> [u8; 16] {
        let mut b = aes::Block::try_from(block).unwrap();
        Aes256::new_from_slice(key).unwrap().decrypt_block(&mut b);
        b.into()
    }

    /// AES-256-CBC decryption without padding.
    fn cbc_decrypt(key: &[u8], iv: &[u8], data: &[u8]) -> Vec<u8> {
        let mut prev = iv.to_vec();
        let mut out = Vec::new();
        for chunk in data.chunks_exact(16) {
            let d = decrypt(key, chunk);
            out.extend(d.iter().zip(&prev).map(|(a, b)| a ^ b));
            prev = chunk.to_vec();
        }
        out
    }

    #[test]
    fn revision_6_entries_round_trip() {
        let params = Params {
            user: b"user".to_vec(),
            owner: b"owner".to_vec(),
            p: -3904,
        };
        let (crypt, dict) = Crypt::new(&params, &[7; 32]);
        // A user password checks against `U`, and its intermediate key
        // unwraps `UE` to the file key; likewise the owner password with
        // `O` and `OE`.
        assert_eq!(hash_2b(b"user", &dict.u[32..40], &[]), dict.u[..32]);
        assert_eq!(hash_2b(b"owner", &dict.o[32..40], &dict.u), dict.o[..32]);
        let ik_u = hash_2b(b"user", &dict.u[40..], &[]);
        let ik_o = hash_2b(b"owner", &dict.o[40..], &dict.u);
        let key = cbc_decrypt(&ik_u, &[0; 16], &dict.ue);
        assert_eq!(cbc_decrypt(&ik_o, &[0; 16], &dict.oe), key);
        // `Perms` decrypts to the permissions and `adb`.
        let perms = decrypt(&key, &dict.perms);
        assert_eq!(perms[..4], (-3904i32).to_le_bytes());
        assert_eq!(&perms[8..12], b"Tadb");
        // A stream decrypts with the file key, its IV first.
        let sealed = crypt.stream(Ref::new(3), b"hello");
        let plain = cbc_decrypt(&key, &sealed[..16], &sealed[16..]);
        assert_eq!(&plain[..5], b"hello");
        assert_eq!(&plain[5..], [11; 11]);
        // Deterministic, and another document has another key.
        let (_, again) = Crypt::new(&params, &[7; 32]);
        assert_eq!((again.ue, again.id), (dict.ue, dict.id));
        let (_, other) = Crypt::new(&params, &[8; 32]);
        assert_ne!(other.ue, dict.ue);
        assert_ne!(other.id, dict.id);
    }

    #[test]
    fn the_encrypt_entry_goes_into_the_trailer() {
        let mut pdf = pdf_writer::Pdf::new();
        pdf.catalog(Ref::new(1)).pages(Ref::new(2));
        pdf.pages(Ref::new(2)).count(0);
        pdf.set_file_id((vec![0xAB], vec![0xAB]));
        let plain = pdf.finish();
        let spliced = splice_encrypt(plain.clone(), Ref::new(3)).unwrap();
        let text = String::from_utf8_lossy(&spliced);
        assert!(
            text.contains("trailer\n<<\n  /Size 3\n  /Root 1 0 R\n  /ID [<AB> <AB>]\n  /Encrypt 3 0 R\n>>\nstartxref\n"),
            "{text}"
        );
        // `startxref` still points at the cross-reference table.
        let xref = spliced.windows(6).position(|w| w == b"\nxref\n").unwrap() + 1;
        assert!(
            text.ends_with(&format!("startxref\n{xref}\n%%EOF")),
            "{text}"
        );
        assert!(splice_encrypt(b"%PDF-1.7".to_vec(), Ref::new(3)).is_none());
    }

    #[test]
    fn ivs_differ_by_place_and_content() {
        let params = Params {
            user: Vec::new(),
            owner: b"o".to_vec(),
            p: -4,
        };
        let (mut crypt, _) = Crypt::new(&params, &[1; 32]);
        let a = crypt.string(b"same");
        let b = crypt.string(b"same");
        assert_ne!(a[..16], b[..16]);
        assert_eq!(a.len(), 32);
        assert_ne!(
            crypt.stream(Ref::new(1), b"x")[..16],
            crypt.stream(Ref::new(1), b"y")[..16]
        );
        assert_eq!(
            crypt.stream(Ref::new(1), b"x"),
            crypt.stream(Ref::new(1), b"x")
        );
        // A whole block of padding after a whole number of blocks.
        assert_eq!(crypt.stream(Ref::new(2), &[0; 16]).len(), 48);
    }
}
