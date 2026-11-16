//! Themes: a fixed set of semantic roles mapped to styles (spec F8).
//! Rendering code asks for a [`Role`], never for a color.

use std::collections::HashMap;
use std::fmt;
use std::ops::Range;
use std::str::FromStr;

use ratatui::style::{Color, Modifier, Style};
use toml::Spanned;

use super::tint::{self, Rgb, TINTS};
use toml::de::{DeTable, DeValue};

macro_rules! roles {
    ($($variant:ident => $name:literal),* $(,)?) => {
        /// Every styled element of the UI.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Role { $($variant),* }

        impl Role {
            pub const ALL: &[Role] = &[$(Role::$variant),*];

            /// The role's key in theme files.
            pub fn name(self) -> &'static str {
                match self { $(Role::$variant => $name),* }
            }

            fn from_name(name: &str) -> Option<Role> {
                match name { $($name => Some(Role::$variant),)* _ => None }
            }
        }
    };
}

roles! {
    Text => "text",
    TextMuted => "text_muted",
    Border => "border",
    BorderFocused => "border_focused",
    Title => "title",
    Selection => "selection",
    VisualSelection => "visual_selection",
    UnreadMarker => "unread_marker",
    UnreadText => "unread_text",
    Actor => "actor",
    Timestamp => "timestamp",
    TaskMeta => "task_meta",
    CommentAuthor => "comment_author",
    Statusbar => "statusbar",
    StatusbarMode => "statusbar_mode",
    Keyhint => "keyhint",
    ToastSuccess => "toast_success",
    ToastError => "toast_error",
    PickerMatch => "picker_match",
}

/// A theme file problem, reported as `source:line: message`.
#[derive(Debug, PartialEq, Eq)]
pub struct ThemeError {
    pub source: String,
    pub line: Option<usize>,
    pub message: String,
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "{}:{line}: {}", self.source, self.message),
            None => write!(f, "{}: {}", self.source, self.message),
        }
    }
}

impl std::error::Error for ThemeError {}

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    styles: HashMap<Role, Style>,
    /// Roles that terminal tints may replace: the tint roles of a
    /// `terminal`-based theme that the user's file doesn't set itself.
    tintable: Vec<Role>,
}

impl Theme {
    /// Picks the theme from `NO_COLOR` and `ASN_THEME` (spec F8): `NO_COLOR`
    /// wins, then a built-in name, then a file path, else `terminal`.
    pub fn resolve(env: impl Fn(&str) -> Option<String>) -> Result<Theme, ThemeError> {
        let set = |key| env(key).filter(|v: &String| !v.is_empty());
        if set("NO_COLOR").is_some() {
            return Ok(Theme::no_color());
        }
        let Some(choice) = set("ASN_THEME") else {
            return Ok(Theme::builtin(DEFAULT_BASE).expect("built-in theme"));
        };
        if let Some(theme) = Theme::builtin(&choice) {
            return Ok(theme);
        }
        let text = std::fs::read_to_string(&choice).map_err(|e| ThemeError {
            source: choice.clone(),
            line: None,
            message: format!("cannot read theme file: {e}"),
        })?;
        Theme::from_toml(&choice, &text)
    }

    /// A built-in theme by name (`terminal`, `dark`, `light`).
    pub fn builtin(name: &str) -> Option<Theme> {
        let (_, text) = BUILTINS.iter().find(|(n, _)| *n == name)?;
        let file = parse(name, text).expect("built-in themes are valid (tested)");
        let tintable = if name == DEFAULT_BASE {
            TINTS.iter().map(|(role, _)| *role).collect()
        } else {
            Vec::new()
        };
        Some(Theme {
            styles: file.styles,
            tintable,
        })
    }

    /// Parses a user theme: its roles replace those of its `base` built-in
    /// (default `terminal`). `source` is used in error messages.
    pub fn from_toml(source: &str, text: &str) -> Result<Theme, ThemeError> {
        let file = parse(source, text)?;
        let mut theme = match &file.base {
            None => Theme::builtin(DEFAULT_BASE).expect("built-in theme"),
            Some(base) => Theme::builtin(base.get_ref()).ok_or_else(|| {
                error(
                    source,
                    text,
                    base.span(),
                    format!(
                        "unknown base theme \"{}\" (expected terminal, dark or light)",
                        base.get_ref()
                    ),
                )
            })?,
        };
        theme
            .tintable
            .retain(|role| !file.styles.contains_key(role));
        theme.styles.extend(file.styles);
        Ok(theme)
    }

    /// `NO_COLOR` (no-color.org): no colors at all, while selection, unread
    /// and focus stay visible through modifiers.
    pub fn no_color() -> Theme {
        let mut theme = Theme::builtin(DEFAULT_BASE).expect("built-in theme");
        theme.tintable.clear();
        for style in theme.styles.values_mut() {
            *style = Style::new().add_modifier(style.add_modifier);
        }
        let mut set = |role, modifiers| {
            theme
                .styles
                .insert(role, Style::new().add_modifier(modifiers));
        };
        set(Role::Selection, Modifier::REVERSED);
        set(
            Role::VisualSelection,
            Modifier::REVERSED | Modifier::UNDERLINED,
        );
        set(Role::UnreadMarker, Modifier::BOLD);
        set(Role::UnreadText, Modifier::BOLD);
        set(Role::BorderFocused, Modifier::BOLD);
        set(Role::StatusbarMode, Modifier::REVERSED | Modifier::BOLD);
        theme
    }

    /// Whether this theme has roles that terminal tints would replace.
    pub fn wants_tints(&self) -> bool {
        !self.tintable.is_empty()
    }

    /// Replaces the tintable roles with shades of the terminal's `fg`/`bg`.
    pub fn apply_tints(&mut self, fg: Rgb, bg: Rgb) {
        for (role, amount) in TINTS {
            if self.tintable.contains(role) {
                self.styles
                    .insert(*role, tint::tinted_style(*role, fg, bg, *amount));
            }
        }
    }

    /// [`Theme::resolve`], then tints from the terminal's real colors when the
    /// theme wants them and the terminal answers. Call before entering raw
    /// mode; without an answer the palette-only theme is used.
    pub fn load(env: impl Fn(&str) -> Option<String>) -> Result<Theme, ThemeError> {
        let mut theme = Theme::resolve(env)?;
        if theme.wants_tints()
            && let Some((fg, bg)) = tint::query_terminal_colors()
        {
            theme.apply_tints(fg, bg);
        }
        Ok(theme)
    }

    pub fn style(&self, role: Role) -> Style {
        self.styles.get(&role).copied().unwrap_or_default()
    }
}

const DEFAULT_BASE: &str = "terminal";

/// Built-in theme files, embedded in the binary.
const BUILTINS: &[(&str, &str)] = &[
    ("terminal", include_str!("../../themes/terminal.toml")),
    ("dark", include_str!("../../themes/dark.toml")),
    ("light", include_str!("../../themes/light.toml")),
];

/// A parsed theme file: its optional `base` and the roles it sets.
struct ThemeFile {
    base: Option<Spanned<String>>,
    styles: HashMap<Role, Style>,
}

fn parse(source: &str, text: &str) -> Result<ThemeFile, ThemeError> {
    let doc = DeTable::parse(text).map_err(|e| ThemeError {
        source: source.to_owned(),
        line: e.span().map(|span| line_of(text, span.start)),
        message: e.message().to_owned(),
    })?;
    let mut file = ThemeFile {
        base: None,
        styles: HashMap::new(),
    };
    for (key, value) in doc.get_ref() {
        let name = key.get_ref().as_ref();
        if name == "base" {
            let DeValue::String(base) = value.get_ref() else {
                return Err(error(
                    source,
                    text,
                    value.span(),
                    "base must be a string".into(),
                ));
            };
            file.base = Some(Spanned::new(value.span(), base.to_string()));
            continue;
        }
        let role = Role::from_name(name)
            .ok_or_else(|| error(source, text, key.span(), format!("unknown role \"{name}\"")))?;
        let DeValue::Table(table) = value.get_ref() else {
            return Err(error(
                source,
                text,
                value.span(),
                format!("role \"{name}\" must be a table"),
            ));
        };
        file.styles.insert(role, parse_style(source, text, table)?);
    }
    Ok(file)
}

fn parse_style(source: &str, text: &str, table: &DeTable<'_>) -> Result<Style, ThemeError> {
    let mut style = Style::new();
    for (key, value) in table {
        match key.get_ref().as_ref() {
            "fg" => style = style.fg(parse_color(source, text, value)?),
            "bg" => style = style.bg(parse_color(source, text, value)?),
            "modifiers" => {
                let DeValue::Array(items) = value.get_ref() else {
                    return Err(error(
                        source,
                        text,
                        value.span(),
                        "modifiers must be a list".into(),
                    ));
                };
                for item in items {
                    style = style.add_modifier(parse_modifier(source, text, item)?);
                }
            }
            other => {
                return Err(error(
                    source,
                    text,
                    key.span(),
                    format!("unknown key \"{other}\" (expected fg, bg or modifiers)"),
                ));
            }
        }
    }
    Ok(style)
}

fn parse_color(
    source: &str,
    text: &str,
    value: &Spanned<DeValue<'_>>,
) -> Result<Color, ThemeError> {
    match value.get_ref() {
        DeValue::String(name) => Color::from_str(name).map_err(|_| {
            error(
                source,
                text,
                value.span(),
                format!("unknown color \"{name}\""),
            )
        }),
        _ => Err(error(
            source,
            text,
            value.span(),
            "colors must be strings".into(),
        )),
    }
}

fn parse_modifier(
    source: &str,
    text: &str,
    value: &Spanned<DeValue<'_>>,
) -> Result<Modifier, ThemeError> {
    let modifier = match value.get_ref() {
        DeValue::String(name) => match name.as_ref() {
            "bold" => Some(Modifier::BOLD),
            "dim" => Some(Modifier::DIM),
            "italic" => Some(Modifier::ITALIC),
            "underlined" => Some(Modifier::UNDERLINED),
            "reversed" => Some(Modifier::REVERSED),
            _ => None,
        },
        _ => None,
    };
    modifier.ok_or_else(|| {
        let shown = match value.get_ref() {
            DeValue::String(name) => name.to_string(),
            _ => "non-string".to_owned(),
        };
        error(
            source,
            text,
            value.span(),
            format!("unknown modifier \"{shown}\""),
        )
    })
}

fn error(source: &str, text: &str, span: Range<usize>, message: String) -> ThemeError {
    ThemeError {
        source: source.to_owned(),
        line: Some(line_of(text, span.start)),
        message,
    }
}

/// 1-based line number of byte `offset` in `text`.
fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].matches('\n').count() + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Modifier};

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn every_builtin_theme_file_lists_every_role() {
        for (name, text) in BUILTINS {
            let table: toml::Table = text.parse().unwrap();
            for role in Role::ALL {
                assert!(
                    table.contains_key(role.name()),
                    "{name} lacks {}",
                    role.name()
                );
            }
        }
    }

    #[test]
    fn builtins_parse() {
        for name in ["terminal", "dark", "light"] {
            assert!(Theme::builtin(name).is_some(), "{name}");
        }
        assert!(Theme::builtin("solarized").is_none());
    }

    #[test]
    fn terminal_theme_uses_only_ansi_colors() {
        let theme = Theme::builtin("terminal").unwrap();
        for role in Role::ALL {
            let style = theme.style(*role);
            for color in [style.fg, style.bg].into_iter().flatten() {
                assert!(
                    !matches!(color, Color::Rgb(..) | Color::Indexed(_)),
                    "{} uses {color:?}",
                    role.name()
                );
            }
        }
    }

    #[test]
    fn default_is_the_terminal_theme() {
        assert_eq!(
            Theme::resolve(no_env).unwrap(),
            Theme::builtin("terminal").unwrap()
        );
    }

    #[test]
    fn asn_theme_selects_a_builtin_by_name() {
        let theme = Theme::resolve(|k| (k == "ASN_THEME").then(|| "dark".to_string())).unwrap();

        assert_eq!(theme, Theme::builtin("dark").unwrap());
    }

    #[test]
    fn user_theme_overrides_listed_roles_and_falls_back_to_its_base() {
        let text = "base = \"dark\"\n\n[selection]\nbg = \"#3c3836\"\nmodifiers = [\"bold\"]\n";

        let theme = Theme::from_toml("theme.toml", text).unwrap();
        let dark = Theme::builtin("dark").unwrap();

        assert_eq!(
            theme.style(Role::Selection),
            Style::new()
                .bg(Color::Rgb(0x3c, 0x38, 0x36))
                .add_modifier(Modifier::BOLD)
        );
        assert_eq!(theme.style(Role::Text), dark.style(Role::Text));
    }

    #[test]
    fn user_theme_without_base_builds_on_terminal() {
        let theme = Theme::from_toml("theme.toml", "[unread_marker]\nfg = \"yellow\"\n").unwrap();
        let terminal = Theme::builtin("terminal").unwrap();

        assert_eq!(
            theme.style(Role::UnreadMarker),
            Style::new().fg(Color::Yellow)
        );
        assert_eq!(theme.style(Role::Title), terminal.style(Role::Title));
    }

    #[test]
    fn accepts_ansi_names_hex_and_256_color_indexes() {
        let text = "[text]\nfg = \"light-blue\"\nbg = \"236\"\n[title]\nfg = \"#ff8800\"\n";

        let theme = Theme::from_toml("theme.toml", text).unwrap();

        assert_eq!(
            theme.style(Role::Text),
            Style::new().fg(Color::LightBlue).bg(Color::Indexed(236))
        );
        assert_eq!(
            theme.style(Role::Title),
            Style::new().fg(Color::Rgb(0xff, 0x88, 0x00))
        );
    }

    #[test]
    fn unknown_role_is_an_error_with_its_line() {
        let text = "base = \"dark\"\n\n[text]\nfg = \"red\"\n\n\n[selction]\nbg = \"blue\"\n";

        let err = Theme::from_toml("theme.toml", text).unwrap_err();

        assert_eq!(err.to_string(), "theme.toml:7: unknown role \"selction\"");
    }

    #[test]
    fn unknown_color_is_an_error_with_its_line() {
        let text = "[text]\nfg = \"blurple\"\n";

        let err = Theme::from_toml("theme.toml", text).unwrap_err();

        assert_eq!(err.to_string(), "theme.toml:2: unknown color \"blurple\"");
    }

    #[test]
    fn unknown_modifier_is_an_error_with_its_line() {
        let text = "[text]\nmodifiers = [\"bold\", \"blinking\"]\n";

        let err = Theme::from_toml("theme.toml", text).unwrap_err();

        assert_eq!(
            err.to_string(),
            "theme.toml:2: unknown modifier \"blinking\""
        );
    }

    #[test]
    fn unknown_key_inside_a_role_is_an_error() {
        let text = "[text]\nforeground = \"red\"\n";

        let err = Theme::from_toml("theme.toml", text).unwrap_err();

        assert_eq!(
            err.to_string(),
            "theme.toml:2: unknown key \"foreground\" (expected fg, bg or modifiers)"
        );
    }

    #[test]
    fn unknown_base_is_an_error() {
        let err = Theme::from_toml("theme.toml", "base = \"solarized\"\n").unwrap_err();

        assert_eq!(
            err.to_string(),
            "theme.toml:1: unknown base theme \"solarized\" (expected terminal, dark or light)"
        );
    }

    #[test]
    fn toml_syntax_error_reports_its_line() {
        let err = Theme::from_toml("theme.toml", "[text]\nfg = \"red\n").unwrap_err();

        assert_eq!(err.line, Some(2));
    }

    #[test]
    fn asn_theme_path_loads_the_file() {
        let path = std::env::temp_dir().join(format!("asn-theme-{}.toml", std::process::id()));
        std::fs::write(&path, "[title]\nfg = \"magenta\"\n").unwrap();
        let path_str = path.to_string_lossy().into_owned();

        let theme = Theme::resolve(|k| (k == "ASN_THEME").then(|| path_str.clone())).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(theme.style(Role::Title), Style::new().fg(Color::Magenta));
    }

    #[test]
    fn unreadable_theme_file_names_the_path() {
        let err = Theme::resolve(|k| (k == "ASN_THEME").then(|| "/nonexistent/theme.toml".into()))
            .unwrap_err();

        assert_eq!(err.source, "/nonexistent/theme.toml");
        assert!(err.message.starts_with("cannot read theme file"), "{err}");
    }

    #[test]
    fn no_color_wins_over_asn_theme_and_strips_every_color() {
        let theme = Theme::resolve(|k| match k {
            "NO_COLOR" => Some("1".into()),
            "ASN_THEME" => Some("dark".into()),
            _ => None,
        })
        .unwrap();

        for role in Role::ALL {
            let style = theme.style(*role);
            assert_eq!(
                (style.fg, style.bg),
                (None, None),
                "{} has a color",
                role.name()
            );
        }
    }

    #[test]
    fn no_color_keeps_cues_as_modifiers() {
        let theme = Theme::resolve(|k| (k == "NO_COLOR").then(|| "1".into())).unwrap();

        assert!(
            theme
                .style(Role::Selection)
                .add_modifier
                .contains(Modifier::REVERSED)
        );
        assert!(
            theme
                .style(Role::UnreadText)
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert!(
            theme
                .style(Role::BorderFocused)
                .add_modifier
                .contains(Modifier::BOLD)
        );
    }

    #[test]
    fn empty_no_color_is_ignored() {
        let theme = Theme::resolve(|k| (k == "NO_COLOR").then(String::new)).unwrap();

        assert_eq!(theme, Theme::builtin("terminal").unwrap());
    }

    #[test]
    fn tints_replace_tint_roles_of_the_terminal_theme() {
        let (fg, bg) = ((0xcd, 0xd6, 0xf4), (0x1e, 0x1e, 0x2e));
        let mut theme = Theme::builtin("terminal").unwrap();

        theme.apply_tints(fg, bg);

        for (role, amount) in crate::tui::tint::TINTS {
            assert_eq!(
                theme.style(*role),
                crate::tui::tint::tinted_style(*role, fg, bg, *amount),
                "{}",
                role.name()
            );
        }
        assert_eq!(
            theme.style(Role::Title),
            Theme::builtin("terminal").unwrap().style(Role::Title)
        );
    }

    #[test]
    fn tints_keep_roles_a_user_theme_sets_itself() {
        let mut theme = Theme::from_toml("theme.toml", "[selection]\nbg = \"red\"\n").unwrap();

        theme.apply_tints((0xcd, 0xd6, 0xf4), (0x1e, 0x1e, 0x2e));

        assert_eq!(theme.style(Role::Selection), Style::new().bg(Color::Red));
        assert_ne!(
            theme.style(Role::Statusbar),
            Theme::builtin("terminal").unwrap().style(Role::Statusbar)
        );
    }

    #[test]
    fn fixed_themes_and_no_color_are_never_tinted() {
        for mut theme in [
            Theme::builtin("dark").unwrap(),
            Theme::from_toml("theme.toml", "base = \"light\"\n").unwrap(),
            Theme::no_color(),
        ] {
            let before = theme.clone();
            assert!(!theme.wants_tints());

            theme.apply_tints((0xcd, 0xd6, 0xf4), (0x1e, 0x1e, 0x2e));

            assert_eq!(theme, before);
        }
    }

    #[test]
    fn terminal_based_themes_want_tints() {
        assert!(Theme::builtin("terminal").unwrap().wants_tints());
        assert!(
            Theme::from_toml("theme.toml", "[title]\nfg = \"red\"\n")
                .unwrap()
                .wants_tints()
        );
    }
}
