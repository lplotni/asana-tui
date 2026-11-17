//! Semantic text roles, shared by every presentation.

macro_rules! roles {
    ($($variant:ident => $name:literal),* $(,)?) => {
        /// What a piece of text is (a title, a timestamp, …). Themes map
        /// roles to colors; plain-text output ignores them.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Role { $($variant),* }

        impl Role {
            pub const ALL: &[Role] = &[$(Role::$variant),*];

            /// The role's key in theme files.
            pub fn name(self) -> &'static str {
                match self { $(Role::$variant => $name),* }
            }

            /// The role named `name` in a theme file.
            pub(crate) fn from_name(name: &str) -> Option<Role> {
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
    Celebrate1 => "celebrate_1",
    Celebrate2 => "celebrate_2",
    Celebrate3 => "celebrate_3",
    Celebrate4 => "celebrate_4",
}
