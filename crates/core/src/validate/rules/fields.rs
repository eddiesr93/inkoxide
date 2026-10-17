//! The checks of form fields and notes.

use std::collections::HashSet;
use std::sync::Arc;

use super::{
    Alpha, Checker, MAX_FIELD_NAME, MAX_FIELD_OPTIONS, MAX_FIELD_OPTION_LEN, MAX_FIELD_PARTS,
    MAX_FIELD_PART_LEN, MAX_FIELD_VALUE, MAX_NOTE, MAX_NOTE_AUTHOR,
};
use crate::error::ValidationCode as Code;
use crate::forms::format;
use crate::schema::{Field, FieldFormat, FieldKind, FieldValue, FormatKind, Note};

impl Checker<'_> {
    /// Records a well-formed field `name` and the groups it makes, after
    /// checking it against the names and groups seen so far.
    pub(super) fn field_name(&mut self, f: &Field) {
        // The groups the name is in: what comes before each of its dots.
        let groups = f.name.match_indices('.').map(|(i, _)| &f.name[..i]);
        if let Some(g) = groups.clone().find(|g| self.fields.contains_key(*g)) {
            self.err(
                Code::DuplicateId,
                Some("name"),
                format!("`{g}` is already the name of a field, so it cannot be a group"),
            );
        } else if self.field_groups.contains(&f.name) {
            self.err(
                Code::DuplicateId,
                Some("name"),
                format!(
                    "`{}` is already the name of a group of fields (`{}.…`)",
                    f.name, f.name
                ),
            );
        } else {
            match self.fields.get(&f.name) {
                Some(&k) if k != FieldKind::Radio || f.kind != FieldKind::Radio => self.err(
                    Code::DuplicateId,
                    Some("name"),
                    format!(
                        "field name `{}` is already used (only radio buttons share a name)",
                        f.name
                    ),
                ),
                _ => {
                    self.fields.insert(f.name.clone(), f.kind);
                    for g in groups {
                        if !self.field_groups.contains(g) {
                            self.field_groups.insert(g.to_string());
                        }
                    }
                }
            }
        }
    }

    pub(super) fn field(&mut self, f: &Field) {
        let errors = self.errors.len();
        if f.label
            .as_ref()
            .is_some_and(|l| !(1..=super::MAX_ALT).contains(&l.chars().count()))
        {
            self.err(
                Code::InvalidValue,
                Some("label"),
                format!("a field's label is 1 to {} characters", super::MAX_ALT),
            );
        }
        let parts = f.name.split('.').count();
        if f.name.chars().count() > MAX_FIELD_NAME
            || parts > MAX_FIELD_PARTS
            || f.name
                .split('.')
                .any(|p| p.is_empty() || p.chars().count() > MAX_FIELD_PART_LEN)
        {
            self.err(
                Code::InvalidValue,
                Some("name"),
                format!("a field name is at most {MAX_FIELD_PARTS} parts of 1 to {MAX_FIELD_PART_LEN} characters, separated by `.`, and at most {MAX_FIELD_NAME} characters in all"),
            );
        } else {
            self.field_name(f);
        }
        if f.kind == FieldKind::Radio && f.checked && !self.checked_radios.insert(f.name.clone()) {
            self.err(
                Code::InvalidValue,
                Some("checked"),
                format!(
                    "radio group `{}` already has a checked button; at most one is selected",
                    f.name
                ),
            );
        }
        match (&f.font, f.is_text_like()) {
            (Some(font), _) => {
                self.font_ref(Some("font"), font);
                if self.fonts.get(font).is_some_and(|d| !d.has_outlines()) {
                    self.err(
                        Code::InvalidValue,
                        Some("font"),
                        format!("font `{font}` has colour bitmaps and no outlines; a form field needs a font with outlines"),
                    );
                }
            }
            (None, true) => self.err(
                Code::MissingField,
                Some("font"),
                "text, select and list fields require `font`",
            ),
            _ => {}
        }
        self.font_size(f.size);
        if let Some(c) = &f.color {
            self.color_as(Some("color"), c, Alpha::Ignored);
        }
        let only = |c: &mut Self, set: bool, field: &str, kinds: &str| {
            if set {
                c.err(
                    Code::InvalidValue,
                    Some(field),
                    format!("`{field}` only applies to {kinds} fields"),
                );
            }
        };
        let is = |k: FieldKind| f.kind == k;
        let choice = matches!(f.kind, FieldKind::Select | FieldKind::List);
        only(
            self,
            !is(FieldKind::Text) && (f.multiline || f.password || f.max_length.is_some()),
            if f.multiline {
                "multiline"
            } else if f.password {
                "password"
            } else {
                "maxLength"
            },
            "text",
        );
        only(
            self,
            !choice && !f.options.is_empty(),
            "options",
            "select and list",
        );
        only(
            self,
            !is(FieldKind::List) && f.multi_select,
            "multiSelect",
            "list",
        );
        only(
            self,
            !matches!(f.kind, FieldKind::Checkbox | FieldKind::Radio) && f.checked,
            "checked",
            "checkbox and radio",
        );
        only(
            self,
            is(FieldKind::Checkbox) && f.value.is_some(),
            "value",
            "text, select, list and radio",
        );
        if f.options.len() > MAX_FIELD_OPTIONS {
            self.err(
                Code::InvalidValue,
                Some("options"),
                format!("a field has at most {MAX_FIELD_OPTIONS} options"),
            );
        }
        for (i, o) in f.options.iter().enumerate() {
            if o.chars().count() > MAX_FIELD_OPTION_LEN {
                self.at("options", |c| {
                    c.at_index(i, |c| {
                        c.err(
                            Code::InvalidValue,
                            None,
                            format!("an option is at most {MAX_FIELD_OPTION_LEN} characters"),
                        );
                    });
                });
            }
        }
        match &f.value {
            Some(FieldValue::One(v)) if v.chars().count() > MAX_FIELD_VALUE => self.err(
                Code::InvalidValue,
                Some("value"),
                format!("a field value is at most {MAX_FIELD_VALUE} characters"),
            ),
            Some(FieldValue::Many(_)) if !(is(FieldKind::List) && f.multi_select) => self.err(
                Code::InvalidValue,
                Some("value"),
                "an array of values needs a list with `multiSelect`",
            ),
            Some(FieldValue::Many(v)) if v.len() > MAX_FIELD_OPTIONS => self.err(
                Code::InvalidValue,
                Some("value"),
                format!("a field has at most {MAX_FIELD_OPTIONS} values"),
            ),
            _ => {}
        }
        if f.max_length == Some(0) {
            self.err(Code::OutOfRange, Some("maxLength"), "must be at least 1");
        }
        if let (Some(max), Some(v)) = (f.max_length, f.value_str()) {
            if v.chars().count() > max as usize {
                self.err(
                    Code::OutOfRange,
                    Some("value"),
                    "is longer than `maxLength`",
                );
            }
        }
        if let Some(format) = &f.format {
            if !is(FieldKind::Text) || f.multiline || f.password {
                self.err(
                    Code::InvalidValue,
                    Some("format"),
                    "`format` only applies to single-line text fields that are not passwords",
                );
            } else if self.at("format", |c| c.field_format(format)) {
                if let Some(v) = f.value_str() {
                    if format::display(format, v).is_none() {
                        self.err(
                            Code::InvalidValue,
                            Some("value"),
                            format!("`{v}` does not fit the field's format"),
                        );
                    }
                }
            }
        }
        match f.kind {
            FieldKind::Radio if f.value_str().is_none_or(str::is_empty) => {
                if !matches!(f.value, Some(FieldValue::Many(_))) {
                    self.err(
                        Code::MissingField,
                        Some("value"),
                        "a radio button requires a non-empty `value` (what it exports)",
                    );
                }
            }
            FieldKind::Select | FieldKind::List if f.options.is_empty() => self.err(
                Code::MissingField,
                Some("options"),
                "select and list fields require `options`",
            ),
            FieldKind::Select | FieldKind::List => match &f.value {
                Some(FieldValue::One(v)) if !f.options.contains(v) => {
                    self.err(Code::InvalidValue, Some("value"), "is not one of `options`");
                }
                Some(FieldValue::Many(values))
                    if f.multi_select && values.len() <= MAX_FIELD_OPTIONS =>
                {
                    let options: HashSet<&str> = f.options.iter().map(String::as_str).collect();
                    let mut seen = HashSet::new();
                    for (i, v) in values.iter().enumerate() {
                        let message = if !options.contains(v.as_str()) {
                            "is not one of `options`"
                        } else if !seen.insert(v.as_str()) {
                            "is already selected: each option is selected at most once"
                        } else {
                            continue;
                        };
                        self.at("value", |c| {
                            c.at_index(i, |c| c.err(Code::InvalidValue, None, message));
                        });
                    }
                }
                _ => {}
            },
            _ => {}
        }
        if self.errors.len() == errors {
            self.field_glyphs(f);
        }
    }

    /// Checks that the font of valid field `f` has a glyph for every
    /// character its appearance shows: the text a text field shows, and
    /// the options of a select or list. Fields are drawn with their own
    /// font only, without fallback fonts.
    pub(super) fn field_glyphs(&mut self, f: &Field) {
        let Some((font, data)) = f
            .font
            .as_deref()
            .and_then(|id| Some((id, Arc::clone(self.fonts.get(id)?))))
        else {
            return;
        };
        let lacks = |c: &mut Self, at: Option<&str>, text: &str| {
            let Some(ch) = data.lacks(text) else {
                return false;
            };
            c.err(
                Code::InvalidValue,
                at,
                format!(
                    "font `{font}` has no glyph for `{ch}` (U+{:04X}); a field is drawn with its own font only, without fallback fonts",
                    u32::from(ch)
                ),
            );
            true
        };
        if f.kind == FieldKind::Text && !f.password {
            if let Some(v) = f.value_str() {
                if !lacks(self, Some("value"), v) {
                    let shown = f.format.as_deref().and_then(|fmt| format::display(fmt, v));
                    if let Some(shown) = shown {
                        lacks(self, Some("value"), &shown.text);
                    }
                }
            }
        }
        for (i, o) in f.options.iter().enumerate() {
            self.at("options", |c| c.at_index(i, |c| lacks(c, None, o)));
        }
    }

    /// Checks a text field's format; true when it is valid.
    pub(super) fn field_format(&mut self, f: &FieldFormat) -> bool {
        let errors = self.errors.len();
        let (number, percent) = (f.kind == FormatKind::Number, f.kind == FormatKind::Percent);
        let dated = matches!(f.kind, FormatKind::Date | FormatKind::Time);
        for (key, set, applies, kinds) in [
            (
                "decimals",
                f.decimals.is_some(),
                number || percent,
                "number and percent",
            ),
            (
                "separators",
                f.separators.is_some(),
                number || percent,
                "number and percent",
            ),
            ("negative", f.negative.is_some(), number, "number"),
            ("currency", f.currency.is_some(), number, "number"),
            (
                "currencyBefore",
                f.currency_before.is_some(),
                number,
                "number",
            ),
            ("pattern", f.pattern.is_some(), dated, "date and time"),
        ] {
            if set && !applies {
                self.err(
                    Code::InvalidValue,
                    Some(key),
                    format!("`{key}` only applies to {kinds} formats"),
                );
            }
        }
        if f.decimals.is_some_and(|d| d > format::MAX_DECIMALS) {
            self.err(
                Code::OutOfRange,
                Some("decimals"),
                format!("decimals must be within 0..={}", format::MAX_DECIMALS),
            );
        }
        if f.currency
            .as_deref()
            .is_some_and(|c| !format::currency_ok(c))
        {
            self.err(
                Code::InvalidValue,
                Some("currency"),
                format!(
                    "a currency is at most {} characters, without control characters, quotes or backslashes",
                    format::MAX_CURRENCY
                ),
            );
        }
        match &f.pattern {
            Some(p) if dated => {
                if let Err(e) = format::check_pattern(p, f.kind) {
                    self.err(Code::InvalidValue, Some("pattern"), e);
                }
            }
            None if dated => self.err(
                Code::MissingField,
                Some("pattern"),
                "date and time formats require `pattern`",
            ),
            _ => {}
        }
        self.errors.len() == errors
    }

    /// A node's `role`: one its type can have. A text is a heading, a
    /// paragraph, a caption, a label, a quote or decoration; an image a
    /// figure or decoration; a box anything.
    pub(super) fn role(&mut self, role: crate::schema::Role, kind: &crate::schema::NodeKind) {
        use crate::schema::{NodeKind, Role};
        let ok = match kind {
            NodeKind::Box { .. } => true,
            NodeKind::Text { .. } => {
                role.heading().is_some()
                    || matches!(
                        role,
                        Role::P | Role::Caption | Role::Label | Role::BlockQuote | Role::Artifact
                    )
            }
            NodeKind::Image { .. } => matches!(role, Role::Figure | Role::Artifact),
            _ => false,
        };
        if !ok {
            self.err(
                Code::InvalidValue,
                Some("role"),
                "a text takes h1 to h6, p, caption, label, blockQuote or artifact; an image figure or artifact",
            );
        }
    }

    pub(super) fn note(&mut self, n: &Note) {
        if !(1..=MAX_NOTE).contains(&n.contents.chars().count()) {
            self.err(
                Code::InvalidValue,
                Some("contents"),
                format!("a note's contents are 1 to {MAX_NOTE} characters"),
            );
        }
        if n.author
            .as_ref()
            .is_some_and(|a| a.chars().count() > MAX_NOTE_AUTHOR)
        {
            self.err(
                Code::InvalidValue,
                Some("author"),
                format!("a note's author is at most {MAX_NOTE_AUTHOR} characters"),
            );
        }
        if let Some(c) = &n.color {
            self.color_as(Some("color"), c, Alpha::Ignored);
        }
    }
}
