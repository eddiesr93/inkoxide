//! The encryption option: passwords and permissions, and their checks.

use std::fmt;

use crate::error::{ValidationCode, ValidationError};
use crate::pdf::crypt::{Params, MAX_PASSWORD};

/// Encrypts the PDF with AES-256 (PDF 2.0, revision 6): opening it needs
/// the user password, and the owner password lifts the [`Permissions`].
///
/// At least one password is needed. Passwords are printable ASCII, at
/// most 127 bytes. Without an owner password, the user password is also
/// the owner password, so whoever opens the file has every permission;
/// `permissions` therefore need an owner password that differs from the
/// user password. Without a user password, anyone opens the file, and the
/// permissions hold for whoever lacks the owner password.
///
/// `Debug` prints `<redacted>` for the passwords.
///
/// ```
/// use inkoxide::{Encryption, Permissions, Printing};
///
/// let encryption = Encryption::new()
///     .with_user_password("open sesame")
///     .with_owner_password("change me")
///     .with_permissions(Permissions::new().with_printing(Printing::HighResolution));
/// assert!(!format!("{encryption:?}").contains("sesame"));
/// ```
#[derive(Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Encryption {
    /// The password that opens the file; `None` or empty: none.
    pub user_password: Option<String>,
    /// The password that lifts the permissions; `None` or empty: the user
    /// password.
    pub owner_password: Option<String>,
    /// What a reader without the owner password may do; `None`:
    /// [`Permissions::default`]. Needs an owner password other than the
    /// user password.
    pub permissions: Option<Permissions>,
}

impl fmt::Debug for Encryption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redacted = |p: &Option<String>| p.as_ref().map(|_| Redacted);
        f.debug_struct("Encryption")
            .field("user_password", &redacted(&self.user_password))
            .field("owner_password", &redacted(&self.owner_password))
            .field("permissions", &self.permissions)
            .finish()
    }
}

/// Prints `<redacted>` in place of a secret.
struct Redacted;

impl fmt::Debug for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

impl Encryption {
    /// No passwords yet, and the default permissions.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets [`user_password`](Self::user_password).
    #[must_use]
    pub fn with_user_password(mut self, password: impl Into<String>) -> Self {
        self.user_password = Some(password.into());
        self
    }

    /// Sets [`owner_password`](Self::owner_password).
    #[must_use]
    pub fn with_owner_password(mut self, password: impl Into<String>) -> Self {
        self.owner_password = Some(password.into());
        self
    }

    /// Sets [`permissions`](Self::permissions).
    #[must_use]
    pub fn with_permissions(mut self, permissions: Permissions) -> Self {
        self.permissions = Some(permissions);
        self
    }

    /// The checked passwords and permission bits. Errors name the option,
    /// never a password.
    pub(crate) fn params(&self) -> Result<Params, ValidationError> {
        let invalid = |path: &str, message: &str| ValidationError {
            code: ValidationCode::InvalidValue,
            path: format!("options.encryption{path}"),
            message: message.into(),
        };
        let password = |p: &Option<String>, path: &str| -> Result<Vec<u8>, ValidationError> {
            let p = p.as_deref().unwrap_or_default();
            if p.len() > MAX_PASSWORD || !p.bytes().all(|b| matches!(b, 0x20..=0x7E)) {
                return Err(invalid(
                    path,
                    &format!("a password is printable ASCII, at most {MAX_PASSWORD} bytes"),
                ));
            }
            Ok(p.as_bytes().to_vec())
        };
        let user = password(&self.user_password, ".userPassword")?;
        let owner = password(&self.owner_password, ".ownerPassword")?;
        if user.is_empty() && owner.is_empty() {
            return Err(invalid("", "encryption needs a user or an owner password"));
        }
        if self.permissions.is_some() && (owner.is_empty() || owner == user) {
            return Err(invalid(
                ".permissions",
                "permissions need an owner password other than the user password, which would lift them",
            ));
        }
        let owner = if owner.is_empty() {
            user.clone()
        } else {
            owner
        };
        Ok(Params {
            user,
            owner,
            p: self.permissions.unwrap_or_default().bits(),
        })
    }
}

/// What a reader who opens an encrypted file without the owner password
/// may do. Every permission is off by default, except
/// `content_accessibility`, which PDF 2.0 asks to be always on.
/// Readers enforce these; the file cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Permissions {
    /// Printing, and at what quality (default `None`: no printing).
    pub printing: Option<Printing>,
    /// Changing the document other than by annotations and form fields.
    pub modifying: bool,
    /// Copying or extracting text and graphics.
    pub copying: bool,
    /// Adding or changing annotations, and filling in form fields.
    pub annotating: bool,
    /// Filling in form fields, even without `annotating`.
    pub filling_forms: bool,
    /// Extracting text and graphics for accessibility (default `true`).
    pub content_accessibility: bool,
    /// Inserting, rotating or deleting pages, and changing bookmarks.
    pub document_assembly: bool,
}

impl Default for Permissions {
    fn default() -> Self {
        Self {
            printing: None,
            modifying: false,
            copying: false,
            annotating: false,
            filling_forms: false,
            content_accessibility: true,
            document_assembly: false,
        }
    }
}

/// How well a reader without the owner password may print.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Printing {
    /// Printing, possibly as a low-resolution image.
    LowResolution,
    /// Printing at full quality.
    HighResolution,
}

impl Permissions {
    /// [`Permissions::default`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets [`printing`](Self::printing).
    #[must_use]
    pub fn with_printing(mut self, printing: impl Into<Option<Printing>>) -> Self {
        self.printing = printing.into();
        self
    }

    /// Sets [`modifying`](Self::modifying).
    #[must_use]
    pub fn with_modifying(mut self, on: bool) -> Self {
        self.modifying = on;
        self
    }

    /// Sets [`copying`](Self::copying).
    #[must_use]
    pub fn with_copying(mut self, on: bool) -> Self {
        self.copying = on;
        self
    }

    /// Sets [`annotating`](Self::annotating).
    #[must_use]
    pub fn with_annotating(mut self, on: bool) -> Self {
        self.annotating = on;
        self
    }

    /// Sets [`filling_forms`](Self::filling_forms).
    #[must_use]
    pub fn with_filling_forms(mut self, on: bool) -> Self {
        self.filling_forms = on;
        self
    }

    /// Sets [`content_accessibility`](Self::content_accessibility).
    #[must_use]
    pub fn with_content_accessibility(mut self, on: bool) -> Self {
        self.content_accessibility = on;
        self
    }

    /// Sets [`document_assembly`](Self::document_assembly).
    #[must_use]
    pub fn with_document_assembly(mut self, on: bool) -> Self {
        self.document_assembly = on;
        self
    }

    /// `/P` (ISO 32000-2, table 22): bits 1 and 2 clear, the reserved
    /// bits 7, 8 and 13 to 32 set, and bit 3 (print), 4 (modify), 5
    /// (copy), 6 (annotate), 9 (fill forms), 10 (accessibility), 11
    /// (assemble) and 12 (print at full quality) for what is allowed.
    pub(crate) fn bits(self) -> i32 {
        let mut p: u32 = 0xFFFF_F0C0;
        let mut set = |on: bool, bit: u32| {
            if on {
                p |= 1 << (bit - 1);
            }
        };
        set(self.printing.is_some(), 3);
        set(self.modifying, 4);
        set(self.copying, 5);
        set(self.annotating, 6);
        set(self.filling_forms, 9);
        set(self.content_accessibility, 10);
        set(self.document_assembly, 11);
        set(self.printing == Some(Printing::HighResolution), 12);
        p as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_bits() {
        assert_eq!(Permissions::default().bits() as u32, 0xFFFF_F2C0);
        let all = Permissions::new()
            .with_printing(Printing::HighResolution)
            .with_modifying(true)
            .with_copying(true)
            .with_annotating(true)
            .with_filling_forms(true)
            .with_document_assembly(true);
        assert_eq!(all.bits(), -4);
        let low = Permissions::new()
            .with_printing(Printing::LowResolution)
            .with_content_accessibility(false);
        assert_eq!(low.bits() as u32, 0xFFFF_F0C4);
    }

    #[test]
    fn passwords_are_checked_and_never_shown() {
        let secret = "s3cret-marker";
        let path = |e: Encryption| e.params().unwrap_err().path;
        assert_eq!(path(Encryption::new()), "options.encryption");
        assert_eq!(
            path(Encryption::new().with_user_password("")),
            "options.encryption"
        );
        let long = format!("{secret}{}", "x".repeat(MAX_PASSWORD));
        for (e, at) in [
            (
                Encryption::new().with_user_password(long.clone()),
                "options.encryption.userPassword",
            ),
            (
                Encryption::new()
                    .with_user_password("u")
                    .with_owner_password(format!("{secret}é")),
                "options.encryption.ownerPassword",
            ),
            (
                Encryption::new().with_user_password(format!("{secret}\n")),
                "options.encryption.userPassword",
            ),
            (
                Encryption::new()
                    .with_user_password(secret)
                    .with_permissions(Permissions::new()),
                "options.encryption.permissions",
            ),
            (
                Encryption::new()
                    .with_user_password(secret)
                    .with_owner_password(secret)
                    .with_permissions(Permissions::new()),
                "options.encryption.permissions",
            ),
        ] {
            assert!(!format!("{e:?}").contains(secret));
            let err = e.params().unwrap_err();
            assert_eq!(err.path, at);
            assert!(!err.to_string().contains(secret), "{err}");
        }
        // The owner password defaults to the user password.
        let p = Encryption::new().with_user_password("u").params().unwrap();
        assert_eq!(
            (p.user.as_slice(), p.owner.as_slice()),
            (&b"u"[..], &b"u"[..])
        );
        let p = Encryption::new().with_owner_password("o").params().unwrap();
        assert_eq!(
            (p.user.as_slice(), p.owner.as_slice()),
            (&b""[..], &b"o"[..])
        );
        let full = Encryption::new()
            .with_user_password(secret)
            .with_owner_password(secret);
        assert!(!format!("{:?}", full.params().unwrap()).contains(secret));
        assert_eq!(
            format!("{full:?}"),
            "Encryption { user_password: Some(<redacted>), owner_password: Some(<redacted>), permissions: None }"
        );
    }
}
