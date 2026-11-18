//! Shades derived from the terminal's real foreground/background (spec F8):
//! the background blended toward the foreground, so the result fits light and
//! dark terminal themes alike.

use std::time::Duration;

use ratatui::style::{Color, Style};

use super::theme::Role;

/// How long to wait for the terminal to answer the color query.
pub const QUERY_TIMEOUT: Duration = Duration::from_millis(100);

/// An 8-bit RGB color.
pub type Rgb = (u8, u8, u8);

/// The roles tints replace, with their blend amount (background → foreground).
pub const TINTS: &[(Role, f32)] = &[
    (Role::Selection, 0.12),
    (Role::VisualSelection, 0.20),
    (Role::Statusbar, 0.06),
    (Role::Border, 0.35),
];

/// `bg` moved `amount` (0.0–1.0) of the way toward `fg`.
pub fn blend(bg: Rgb, fg: Rgb, amount: f32) -> Color {
    let mix = |b: u8, f: u8| (f32::from(b) + (f32::from(f) - f32::from(b)) * amount).round() as u8;
    Color::Rgb(mix(bg.0, fg.0), mix(bg.1, fg.1), mix(bg.2, fg.2))
}

/// The style a tinted role gets. Borders tint the foreground, everything else
/// the background.
pub fn tinted_style(role: Role, fg: Rgb, bg: Rgb, amount: f32) -> Style {
    let color = blend(bg, fg, amount);
    match role {
        Role::Border => Style::new().fg(color),
        _ => Style::new().bg(color),
    }
}

/// Asks the terminal for its foreground and background. `None` when there is
/// no terminal, it doesn't support the query, or it doesn't answer in time.
/// Must run before the TUI enters raw mode and the alternate screen.
pub fn query_terminal_colors() -> Option<(Rgb, Rgb)> {
    let mut options = terminal_colorsaurus::QueryOptions::default();
    options.timeout = QUERY_TIMEOUT;
    let palette = terminal_colorsaurus::color_palette(options).ok()?;
    Some((
        palette.foreground.scale_to_8bit(),
        palette.background.scale_to_8bit(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    const DARK_BG: Rgb = (0x1e, 0x1e, 0x2e);
    const DARK_FG: Rgb = (0xcd, 0xd6, 0xf4);
    const LIGHT_BG: Rgb = (0xfa, 0xfa, 0xfa);
    const LIGHT_FG: Rgb = (0x38, 0x3a, 0x42);

    #[test]
    fn blending_a_dark_background_lightens_it_toward_the_foreground() {
        assert_eq!(blend(DARK_BG, DARK_FG, 0.12), Color::Rgb(51, 52, 70));
    }

    #[test]
    fn blending_a_light_background_darkens_it_toward_the_foreground() {
        assert_eq!(blend(LIGHT_BG, LIGHT_FG, 0.12), Color::Rgb(227, 227, 228));
    }

    #[test]
    fn blend_endpoints_are_the_inputs() {
        assert_eq!(blend(DARK_BG, DARK_FG, 0.0), Color::Rgb(0x1e, 0x1e, 0x2e));
        assert_eq!(blend(DARK_BG, DARK_FG, 1.0), Color::Rgb(0xcd, 0xd6, 0xf4));
    }

    #[test]
    fn border_tint_is_a_foreground_color_and_others_are_backgrounds() {
        assert_eq!(
            tinted_style(Role::Border, DARK_FG, DARK_BG, 0.35),
            Style::new().fg(blend(DARK_BG, DARK_FG, 0.35))
        );
        assert_eq!(
            tinted_style(Role::Selection, DARK_FG, DARK_BG, 0.12),
            Style::new().bg(blend(DARK_BG, DARK_FG, 0.12))
        );
    }

    #[test]
    fn query_without_a_terminal_gives_up_quickly() {
        // The query is only sent when a stdio stream is a terminal. That's
        // the case for `cargo test` run in a terminal (libtest captures
        // output internally, so stdout/stderr stay the terminal), which may
        // answer with its real colors. Without one (CI), there's no answer.
        use std::io::IsTerminal;
        let has_terminal = std::io::stdin().is_terminal()
            || std::io::stdout().is_terminal()
            || std::io::stderr().is_terminal();
        let started = Instant::now();

        let colors = query_terminal_colors();

        if !has_terminal {
            assert_eq!(colors, None);
        }
        assert!(
            started.elapsed() < QUERY_TIMEOUT * 3,
            "{:?}",
            started.elapsed()
        );
    }
}
