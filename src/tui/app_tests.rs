//! Tests for the TUI state machine ([`super`]).

use super::*;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

/// `n` notifications with GIDs "0", "1", …; `read` decides each one's state.
fn inbox(n: usize, read: impl Fn(usize) -> bool) -> Vec<Notification> {
    let text = std::fs::read_to_string("tests/fixtures/notifications_page.json").unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    let template: Notification = serde_json::from_value(value["data"][0].clone()).unwrap();
    (0..n)
        .map(|i| {
            let mut n = Notification {
                gid: i.to_string(),
                is_read: read(i),
                ..template.clone()
            };
            n.work_objects[0].gid = format!("t{i}");
            n
        })
        .collect()
}

fn loaded_with(notifications: Vec<Notification>, next: Option<&str>, list_rows: usize) -> App {
    let (mut app, _) = App::new();
    app.update(Msg::Resize(Sizes {
        list_rows,
        detail_width: 60,
        detail_rows: 20,
    }));
    app.update(Msg::InboxLoaded(Ok((
        notifications,
        next.map(str::to_owned),
    ))));
    app
}

fn loaded(n: usize, list_rows: usize) -> App {
    loaded_with(inbox(n, |_| false), None, list_rows)
}

/// Every effect of typing `typed`.
fn keys_all(app: &mut App, typed: &str) -> Vec<Effect> {
    typed
        .chars()
        .flat_map(|c| {
            app.update(Msg::Key(KeyEvent::new(
                KeyCode::Char(c),
                KeyModifiers::NONE,
            )))
        })
        .collect()
}

/// The effects of typing `typed`, minus detail scheduling (tested apart).
fn keys(app: &mut App, typed: &str) -> Vec<Effect> {
    ignoring_detail(keys_all(app, typed))
}

fn ignoring_detail(effects: Vec<Effect>) -> Vec<Effect> {
    effects
        .into_iter()
        .filter(|e| !matches!(e, Effect::ScheduleDetail { .. }))
        .collect()
}

fn key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> Vec<Effect> {
    app.update(Msg::Key(KeyEvent::new(code, modifiers)))
}

fn selected_gid(app: &App) -> &str {
    &app.selected().unwrap().gid
}

fn visible_gids(app: &App) -> Vec<&str> {
    app.visible().map(|n| n.gid.as_str()).collect()
}

// --- Loading -------------------------------------------------------------

#[test]
fn starts_loading_the_first_page_showing_all_like_the_web_inbox() {
    let (app, effects) = App::new();

    assert!(app.loading);
    assert!(!app.unread_only);
    assert_eq!(
        effects,
        [Effect::LoadInbox {
            target: INITIAL_LOAD
        }]
    );
}

#[test]
fn read_and_unread_notifications_are_both_shown_by_default() {
    let app = loaded_with(inbox(4, |i| i % 2 == 0), None, 10);

    assert_eq!(visible_gids(&app), ["0", "1", "2", "3"]);
}

#[test]
fn unread_only_hides_read_notifications() {
    let mut app = loaded_with(inbox(4, |i| i % 2 == 0), None, 10);

    keys(&mut app, "f");

    assert_eq!(visible_gids(&app), ["1", "3"]);
}

#[test]
fn load_error_with_nothing_loaded_is_shown_as_the_error() {
    let (mut app, _) = App::new();

    app.update(Msg::InboxLoaded(Err("Could not reach Asana".into())));

    assert!(!app.loading);
    assert_eq!(app.error.as_deref(), Some("Could not reach Asana"));
}

// --- Navigation ----------------------------------------------------------

#[test]
fn j_and_k_move_and_stop_at_the_edges() {
    let mut app = loaded(3, 10);

    keys(&mut app, "k");
    assert_eq!(app.cursor, 0);
    keys(&mut app, "jjjj");
    assert_eq!(app.cursor, 2);
    keys(&mut app, "k");
    assert_eq!(app.cursor, 1);
}

#[test]
fn counts_multiply_moves_and_clamp() {
    let mut app = loaded(10, 10);

    keys(&mut app, "5j");
    assert_eq!(app.cursor, 5);
    keys(&mut app, "20j");
    assert_eq!(app.cursor, 9);
    keys(&mut app, "3k");
    assert_eq!(app.cursor, 6);
}

#[test]
fn gg_and_capital_g_jump_to_top_bottom_and_line_n() {
    let mut app = loaded(10, 10);

    keys(&mut app, "G");
    assert_eq!(app.cursor, 9);
    keys(&mut app, "gg");
    assert_eq!(app.cursor, 0);
    keys(&mut app, "4G");
    assert_eq!(app.cursor, 3);
    keys(&mut app, "99G");
    assert_eq!(app.cursor, 9);
}

#[test]
fn half_page_moves_half_the_visible_rows() {
    let mut app = loaded(30, 10);

    key(&mut app, KeyCode::Char('d'), KeyModifiers::CONTROL);
    assert_eq!(app.cursor, 5);
    key(&mut app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    assert_eq!(app.cursor, 0);
}

#[test]
fn scrolling_keeps_the_cursor_visible() {
    let mut app = loaded(30, 5);

    keys(&mut app, "7j");
    assert_eq!((app.cursor, app.scroll), (7, 3));
    keys(&mut app, "6k");
    assert_eq!((app.cursor, app.scroll), (1, 1));
    keys(&mut app, "G");
    assert_eq!((app.cursor, app.scroll), (29, 25));
}

#[test]
fn moving_in_an_empty_inbox_does_nothing() {
    let mut app = loaded(0, 5);

    keys(&mut app, "jGk");

    assert_eq!(app.cursor, 0);
    assert!(app.selected().is_none());
}

#[test]
fn q_and_ctrl_c_quit() {
    let mut app = loaded(3, 5);
    keys(&mut app, "q");
    assert!(app.should_quit);

    let mut app = loaded(3, 5);
    key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(app.should_quit);
}

// --- Read state ----------------------------------------------------------

#[test]
fn e_marks_read_and_keeps_the_row_like_the_web_inbox() {
    let mut app = loaded(4, 10);
    keys(&mut app, "j");

    let effects = keys(&mut app, "e");

    assert_eq!(
        effects,
        [Effect::MarkRead {
            gids: vec!["1".into()],
            read: true
        }]
    );
    assert_eq!(visible_gids(&app), ["0", "1", "2", "3"]);
    assert_eq!(selected_gid(&app), "1");
    assert!(app.selected().unwrap().is_read);
}

#[test]
fn in_unread_only_mode_e_removes_the_row_and_lands_on_the_next_item() {
    let mut app = loaded(4, 10);
    keys(&mut app, "fj");

    let effects = keys(&mut app, "e");

    assert_eq!(
        effects,
        [Effect::MarkRead {
            gids: vec!["1".into()],
            read: true
        }]
    );
    assert_eq!(visible_gids(&app), ["0", "2", "3"]);
    assert_eq!(selected_gid(&app), "2");
}

#[test]
fn a_failed_mark_restores_the_row_and_shows_an_error_toast() {
    let mut app = loaded(4, 10);
    keys(&mut app, "fje");

    app.update(Msg::Marked {
        gids: vec!["1".into()],
        read: true,
        result: Err("Could not reach Asana".into()),
    });

    assert_eq!(visible_gids(&app), ["0", "1", "2", "3"]);
    assert!(app.toast.as_ref().unwrap().is_error);
    assert_eq!(
        selected_gid(&app),
        "2",
        "the cursor stays on the item it was on"
    );
}

#[test]
fn only_the_gids_asana_could_not_update_are_restored() {
    let mut app = loaded(4, 10);
    keys(&mut app, "fVje");

    app.update(Msg::Marked {
        gids: vec!["0".into(), "1".into()],
        read: true,
        result: Ok(vec!["1".into()]),
    });

    assert_eq!(visible_gids(&app), ["1", "2", "3"]);
    assert!(app.toast.as_ref().unwrap().is_error);
}

#[test]
fn u_marks_unread_and_x_toggles() {
    let mut app = loaded_with(inbox(2, |_| true), None, 10);

    assert_eq!(
        keys(&mut app, "u"),
        [Effect::MarkRead {
            gids: vec!["0".into()],
            read: false
        }]
    );
    assert_eq!(
        keys(&mut app, "x"),
        [Effect::MarkRead {
            gids: vec!["0".into()],
            read: true
        }]
    );
    assert_eq!(
        keys(&mut app, "x"),
        [Effect::MarkRead {
            gids: vec!["0".into()],
            read: false
        }]
    );
}

#[test]
fn marking_without_a_change_sends_nothing() {
    let mut app = loaded(2, 10);

    assert!(keys(&mut app, "u").is_empty());
}

#[test]
fn visual_mode_marks_the_whole_range_in_one_request_and_ends() {
    let mut app = loaded(5, 10);
    keys(&mut app, "fjVjj");
    assert!(app.in_visual(1) && app.in_visual(3) && !app.in_visual(4));

    let effects = keys(&mut app, "e");

    assert_eq!(
        effects,
        [Effect::MarkRead {
            gids: vec!["1".into(), "2".into(), "3".into()],
            read: true
        }]
    );
    assert_eq!(app.visual_anchor, None);
    assert_eq!(visible_gids(&app), ["0", "4"]);
    assert_eq!(selected_gid(&app), "4");
}

#[test]
fn esc_leaves_visual_mode() {
    let mut app = loaded(5, 10);
    keys(&mut app, "Vj");

    key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!(app.visual_anchor, None);
}

// --- Filter --------------------------------------------------------------

#[test]
fn f_toggles_the_filter_and_keeps_the_selected_notification() {
    let mut app = loaded_with(inbox(6, |i| i % 2 == 0), None, 10);
    keys(&mut app, "3j");
    assert_eq!(selected_gid(&app), "3");

    keys(&mut app, "f");
    assert!(app.unread_only);
    assert_eq!(visible_gids(&app), ["1", "3", "5"]);
    assert_eq!(selected_gid(&app), "3");

    keys(&mut app, "f");
    assert_eq!(selected_gid(&app), "3");
}

// --- Refresh -------------------------------------------------------------

#[test]
fn r_and_the_timer_refresh_at_least_as_much_as_is_loaded() {
    let mut app = loaded(150, 10);

    assert_eq!(keys(&mut app, "R"), [Effect::LoadInbox { target: 150 }]);
    app.update(Msg::InboxLoaded(Ok((inbox(150, |_| false), None))));
    assert_eq!(app.update(Msg::Tick), [Effect::LoadInbox { target: 150 }]);
}

#[test]
fn no_second_refresh_while_one_is_running() {
    let mut app = loaded(5, 10);
    keys(&mut app, "R");

    assert!(app.update(Msg::Tick).is_empty());
    assert!(keys(&mut app, "R").is_empty());
}

#[test]
fn refresh_keeps_the_cursor_on_the_same_notification() {
    let mut app = loaded(10, 10);
    keys(&mut app, "5j");
    assert_eq!(selected_gid(&app), "5");
    keys(&mut app, "R");
    let mut fresh = inbox(10, |_| false);
    fresh.insert(
        0,
        Notification {
            gid: "new".into(),
            ..fresh[0].clone()
        },
    );

    app.update(Msg::InboxLoaded(Ok((fresh, None))));

    assert_eq!(selected_gid(&app), "5");
    assert_eq!(app.cursor, 6);
}

#[test]
fn refresh_moves_to_the_nearest_neighbour_when_the_item_is_gone() {
    let mut app = loaded(10, 10);
    keys(&mut app, "5j");
    keys(&mut app, "R");
    let fresh: Vec<_> = inbox(10, |_| false)
        .into_iter()
        .filter(|n| n.gid != "5")
        .collect();

    app.update(Msg::InboxLoaded(Ok((fresh, None))));

    assert_eq!(selected_gid(&app), "6");
}

// --- Loading more --------------------------------------------------------

#[test]
fn nearing_the_end_loads_the_next_page_exactly_once() {
    let mut app = loaded_with(inbox(30, |_| false), Some("p2"), 10);

    assert!(keys(&mut app, "10j").is_empty());
    assert_eq!(
        keys(&mut app, "10j"),
        [Effect::LoadMore {
            offset: "p2".into()
        }]
    );
    assert!(keys(&mut app, "j").is_empty(), "already loading");

    app.update(Msg::MoreLoaded(Ok((
        inbox(40, |_| false).split_off(30),
        Some("p3".into()),
    ))));
    assert_eq!(app.visible_len(), 40);
    assert_eq!(
        keys(&mut app, "G"),
        [Effect::LoadMore {
            offset: "p3".into()
        }]
    );
}

#[test]
fn a_short_inbox_keeps_loading_until_the_cursor_has_room() {
    let (mut app, _) = App::new();
    app.update(Msg::Resize(Sizes {
        list_rows: 10,
        detail_width: 60,
        detail_rows: 20,
    }));

    let effects = ignoring_detail(app.update(Msg::InboxLoaded(Ok((
        inbox(3, |_| false),
        Some("p2".into()),
    )))));

    assert_eq!(
        effects,
        [Effect::LoadMore {
            offset: "p2".into()
        }]
    );
}

#[test]
fn a_failed_load_more_reloads_from_the_first_page_once() {
    let mut app = loaded_with(inbox(5, |_| false), Some("expired"), 10);

    let effects = app.update(Msg::MoreLoaded(Err("offset expired".into())));
    assert_eq!(
        effects,
        [Effect::LoadInbox {
            target: INITIAL_LOAD
        }]
    );

    app.update(Msg::InboxLoaded(Ok((
        inbox(5, |_| false),
        Some("fresh".into()),
    ))));
    let effects = app.update(Msg::MoreLoaded(Err("offset expired".into())));
    assert_eq!(
        effects,
        [Effect::LoadInbox {
            target: INITIAL_LOAD
        }],
        "a successful reload resets recovery"
    );

    let effects = app.update(Msg::MoreLoaded(Err("still failing".into())));
    assert!(effects.is_empty());
    assert!(app.toast.as_ref().unwrap().is_error);
}

#[test]
fn a_key_press_clears_the_toast() {
    let mut app = loaded(3, 10);
    app.toast = Some(Toast {
        text: "x".into(),
        is_error: false,
    });

    keys(&mut app, "j");

    assert_eq!(app.toast, None);
}

// --- Task detail ---------------------------------------------------------

fn task_detail() -> TaskDetail {
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(format!("tests/fixtures/{name}")).unwrap())
            .unwrap()
    };
    let task = serde_json::from_value(read("task_full.json")["data"].clone()).unwrap();
    let stories: Vec<crate::api::tasks::TaskStory> =
        serde_json::from_value(read("stories_thread.json")["data"].clone()).unwrap();
    let (comments, activity): (Vec<_>, Vec<_>) = stories
        .into_iter()
        .partition(crate::api::tasks::TaskStory::is_comment);
    TaskDetail {
        task,
        comments,
        activity_count: activity.len(),
    }
}

fn schedules(effects: &[Effect]) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::ScheduleDetail { gid } => Some(gid.as_str()),
            _ => None,
        })
        .collect()
}

fn loaded_detail(app: &mut App, gid: &str) {
    app.update(Msg::DetailDue { gid: gid.into() });
    app.update(Msg::DetailLoaded {
        gid: gid.into(),
        result: Ok(Box::new(task_detail())),
    });
}

#[test]
fn loading_the_inbox_schedules_the_first_tasks_details() {
    let (mut app, _) = App::new();

    let effects = app.update(Msg::InboxLoaded(Ok((inbox(3, |_| false), None))));

    assert_eq!(schedules(&effects), ["t0"]);
    assert_eq!(app.selected_task_gid(), Some("t0"));
}

#[test]
fn moving_fast_over_ten_rows_loads_only_the_task_it_stops_on() {
    let mut app = loaded(20, 10);
    let mut scheduled: Vec<String> = Vec::new();
    for _ in 0..10 {
        scheduled.extend(
            schedules(&keys_all(&mut app, "j"))
                .into_iter()
                .map(str::to_owned),
        );
    }

    let loads: Vec<Effect> = scheduled
        .into_iter()
        .flat_map(|gid| app.update(Msg::DetailDue { gid }))
        .collect();

    assert_eq!(loads, [Effect::LoadDetail { gid: "t10".into() }]);
    assert_eq!(app.selected_detail(), Some(&Detail::Loading));
}

#[test]
fn a_cached_task_is_not_loaded_again() {
    let mut app = loaded(3, 10);
    loaded_detail(&mut app, "t0");
    keys(&mut app, "j");

    let effects = keys_all(&mut app, "k");

    assert!(schedules(&effects).is_empty());
    assert!(matches!(app.selected_detail(), Some(Detail::Loaded(_))));
}

#[test]
fn a_response_for_another_task_does_not_replace_the_selected_one() {
    let mut app = loaded(3, 10);
    app.update(Msg::DetailDue { gid: "t0".into() });

    app.update(Msg::DetailLoaded {
        gid: "t2".into(),
        result: Ok(Box::new(task_detail())),
    });

    assert_eq!(app.selected_detail(), Some(&Detail::Loading));
}

#[test]
fn a_failed_load_is_shown_for_that_task() {
    let mut app = loaded(3, 10);
    app.update(Msg::DetailDue { gid: "t0".into() });

    app.update(Msg::DetailLoaded {
        gid: "t0".into(),
        result: Err("timed out".into()),
    });

    assert_eq!(
        app.selected_detail(),
        Some(&Detail::Failed("timed out".into()))
    );
}

#[test]
fn notifications_about_non_tasks_load_nothing() {
    let (mut app, _) = App::new();
    let mut notifications = inbox(1, |_| false);
    notifications[0].work_objects[0].resource_type = "project".into();

    let effects = app.update(Msg::InboxLoaded(Ok((notifications, None))));

    assert!(schedules(&effects).is_empty());
    assert_eq!(app.selected_task_gid(), None);
}

#[test]
fn l_enter_h_and_esc_move_focus_between_panes() {
    let mut app = loaded(3, 10);

    keys(&mut app, "l");
    assert_eq!(app.focus, Focus::Detail);
    keys(&mut app, "h");
    assert_eq!(app.focus, Focus::List);
    key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.focus, Focus::Detail);
    key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(app.focus, Focus::List);
}

#[test]
fn with_the_detail_focused_motions_scroll_it_instead_of_the_list() {
    let mut app = loaded(3, 10);
    loaded_detail(&mut app, "t0");
    keys(&mut app, "l");

    keys(&mut app, "3j");
    assert_eq!((app.cursor, app.detail_scroll), (0, 3));
    keys(&mut app, "k");
    assert_eq!(app.detail_scroll, 2);
    keys(&mut app, "gg");
    assert_eq!(app.detail_scroll, 0);
}

#[test]
fn capital_g_scrolls_the_detail_to_its_last_page() {
    let mut app = loaded(3, 10);
    loaded_detail(&mut app, "t0");
    keys(&mut app, "l");
    let total = crate::tui::detail::lines(
        &task_detail(),
        &crate::tui::theme::Theme::no_color(),
        60,
        app.now,
    )
    .len();

    keys(&mut app, "G");

    assert_eq!(app.detail_scroll, total - 20);
    keys(&mut app, "j");
    assert_eq!(app.detail_scroll, total - 20, "can't scroll past the end");
}

#[test]
fn selecting_another_notification_resets_the_detail_scroll() {
    let mut app = loaded(3, 10);
    loaded_detail(&mut app, "t0");
    keys(&mut app, "l3jh");

    keys(&mut app, "j");

    assert_eq!(app.detail_scroll, 0);
}

#[test]
fn o_opens_the_loaded_task_in_the_browser() {
    let mut app = loaded(3, 10);
    loaded_detail(&mut app, "t0");

    let effects = keys(&mut app, "o");

    assert_eq!(
        effects,
        [Effect::OpenUrl {
            url: task_detail().task.permalink_url
        }]
    );
}

#[test]
fn refresh_reloads_the_shown_task_without_hiding_it_and_forgets_the_rest() {
    let mut app = loaded(3, 10);
    loaded_detail(&mut app, "t1");
    loaded_detail(&mut app, "t0");
    keys(&mut app, "R");

    let effects = app.update(Msg::InboxLoaded(Ok((inbox(3, |_| false), None))));

    assert_eq!(effects, [Effect::LoadDetail { gid: "t0".into() }]);
    assert!(matches!(app.selected_detail(), Some(Detail::Loaded(_))));
    assert!(!app.details.contains_key("t1"));
}

// --- Review fixes --------------------------------------------------------

#[test]
fn q_in_the_detail_pane_goes_back_instead_of_quitting() {
    let mut app = loaded(3, 10);
    keys(&mut app, "l");

    keys(&mut app, "q");

    assert!(!app.should_quit);
    assert_eq!(app.focus, Focus::List);
}

#[test]
fn q_in_visual_mode_leaves_visual_mode() {
    let mut app = loaded(3, 10);
    keys(&mut app, "Vj");

    keys(&mut app, "q");

    assert!(!app.should_quit);
    assert_eq!(app.visual_anchor, None);
}

#[test]
fn ctrl_c_quits_from_anywhere() {
    let mut app = loaded(3, 10);
    keys(&mut app, "lV");

    key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);

    assert!(app.should_quit);
}

#[test]
fn o_opens_a_project_notification_by_its_url() {
    let (mut app, _) = App::new();
    let mut notifications = inbox(1, |_| false);
    notifications[0].work_objects[0].resource_type = "project".into();
    notifications[0].work_objects[0].gid = "555".into();
    app.update(Msg::InboxLoaded(Ok((notifications, None))));

    assert_eq!(
        keys(&mut app, "o"),
        [Effect::OpenUrl {
            url: "https://app.asana.com/0/555".into()
        }]
    );
}

#[test]
fn o_opens_a_task_even_before_its_details_load() {
    let mut app = loaded(3, 10);

    assert_eq!(
        keys(&mut app, "o"),
        [Effect::OpenUrl {
            url: "https://app.asana.com/0/0/t0".into()
        }]
    );
}

#[test]
fn o_on_an_unknown_kind_of_item_explains_it_cannot_open_it() {
    let (mut app, _) = App::new();
    let mut notifications = inbox(1, |_| false);
    notifications[0].work_objects[0].resource_type = "goal".into();
    app.update(Msg::InboxLoaded(Ok((notifications, None))));

    assert!(keys(&mut app, "o").is_empty());
    assert!(app.toast.as_ref().unwrap().text.contains("goal"));
}

#[test]
fn a_refresh_that_started_before_a_mark_does_not_undo_it() {
    let mut app = loaded(4, 10);
    keys(&mut app, "f");
    keys(&mut app, "R");
    keys(&mut app, "je");
    assert_eq!(visible_gids(&app), ["0", "2", "3"]);

    // The refresh was built before markRead reached Asana: "1" is unread.
    app.update(Msg::InboxLoaded(Ok((inbox(4, |_| false), None))));

    assert_eq!(visible_gids(&app), ["0", "2", "3"]);
    app.update(Msg::Marked {
        gids: vec!["1".into()],
        read: true,
        result: Ok(vec![]),
    });
    app.update(Msg::InboxLoaded(Ok((inbox(4, |i| i == 1), None))));
    assert_eq!(visible_gids(&app), ["0", "2", "3"]);
}

#[test]
fn no_refresh_starts_while_more_items_are_loading() {
    let mut app = loaded_with(inbox(30, |_| false), Some("p2"), 10);
    keys(&mut app, "25j");
    assert!(app.loading_more);

    assert!(keys(&mut app, "R").is_empty());
    assert!(app.update(Msg::Tick).is_empty());
}

#[test]
fn marking_a_visual_range_lands_on_the_item_after_it() {
    let mut app = loaded(6, 10);
    keys(&mut app, "fjVj");

    keys(&mut app, "e");

    assert_eq!(visible_gids(&app), ["0", "3", "4", "5"]);
    assert_eq!(selected_gid(&app), "3");
}

#[test]
fn a_shorter_reload_of_the_shown_task_keeps_the_scroll_in_range() {
    let mut app = loaded(3, 10);
    loaded_detail(&mut app, "t0");
    keys(&mut app, "lG");
    assert!(app.detail_scroll > 0);
    let mut short = task_detail();
    short.comments.clear();
    short.task.notes = String::new();

    app.update(Msg::DetailLoaded {
        gid: "t0".into(),
        result: Ok(Box::new(short)),
    });

    assert_eq!(app.detail_scroll, 0);
}
// --- Comments ------------------------------------------------------------

fn comments_of(app: &App, gid: &str) -> Vec<(String, String)> {
    match app.details.get(gid) {
        Some(Detail::Loaded(d)) => d
            .comments
            .iter()
            .map(|c| (c.gid.clone(), c.text.clone().unwrap_or_default()))
            .collect(),
        _ => panic!("detail for {gid} not loaded"),
    }
}

#[test]
fn c_and_tab_c_open_the_editor_for_the_selected_task() {
    let mut app = loaded(3, 10);

    assert_eq!(
        keys(&mut app, "c"),
        [Effect::EditComment {
            task_gid: "t0".into(),
            initial: String::new()
        }]
    );
    key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(
        keys(&mut app, "c"),
        [Effect::EditComment {
            task_gid: "t0".into(),
            initial: String::new()
        }]
    );
}

#[test]
fn c_on_a_non_task_explains_why_nothing_happens() {
    let (mut app, _) = App::new();
    let mut notifications = inbox(1, |_| false);
    notifications[0].work_objects[0].resource_type = "project".into();
    app.update(Msg::InboxLoaded(Ok((notifications, None))));

    assert!(keys(&mut app, "c").is_empty());
    assert!(app.toast.is_some());
}

#[test]
fn a_cancelled_editor_posts_nothing() {
    let mut app = loaded(3, 10);

    let effects = app.update(Msg::CommentWritten {
        task_gid: "t0".into(),
        text: None,
    });

    assert!(effects.is_empty());
}

#[test]
fn a_written_comment_appears_at_once_and_is_posted() {
    let mut app = loaded(3, 10);
    loaded_detail(&mut app, "t0");
    let before = comments_of(&app, "t0").len();

    let effects = app.update(Msg::CommentWritten {
        task_gid: "t0".into(),
        text: Some("Ship it\n".into()),
    });

    assert_eq!(
        effects,
        [Effect::PostComment {
            task_gid: "t0".into(),
            local_gid: "local-1".into(),
            text: "Ship it\n".into()
        }]
    );
    let comments = comments_of(&app, "t0");
    assert_eq!(comments.len(), before + 1);
    assert_eq!(
        comments.last().unwrap(),
        &("local-1".to_string(), "Ship it\n".to_string())
    );
}

#[test]
fn the_posted_comment_replaces_the_local_one() {
    let mut app = loaded(3, 10);
    loaded_detail(&mut app, "t0");
    app.update(Msg::CommentWritten {
        task_gid: "t0".into(),
        text: Some("Ship it\n".into()),
    });
    let mut stored = task_detail().comments[0].clone();
    stored.gid = "999".into();
    stored.text = Some("Ship it\n".into());

    app.update(Msg::CommentPosted {
        task_gid: "t0".into(),
        local_gid: "local-1".into(),
        result: Ok(Box::new(stored)),
    });

    let comments = comments_of(&app, "t0");
    assert_eq!(comments.last().unwrap().0, "999");
    assert!(!comments.iter().any(|(gid, _)| gid == "local-1"));
    assert!(!app.toast.as_ref().unwrap().is_error);
}

#[test]
fn a_failed_post_removes_the_comment_and_keeps_the_text_as_a_draft() {
    let mut app = loaded(3, 10);
    loaded_detail(&mut app, "t0");
    let before = comments_of(&app, "t0").len();
    app.update(Msg::CommentWritten {
        task_gid: "t0".into(),
        text: Some("Ship it\n".into()),
    });

    app.update(Msg::CommentPosted {
        task_gid: "t0".into(),
        local_gid: "local-1".into(),
        result: Err("timed out".into()),
    });

    assert_eq!(comments_of(&app, "t0").len(), before);
    assert!(app.toast.as_ref().unwrap().is_error);
    assert_eq!(
        keys(&mut app, "c"),
        [Effect::EditComment {
            task_gid: "t0".into(),
            initial: "Ship it\n".into()
        }]
    );
}

#[test]
fn commenting_works_before_the_details_have_loaded() {
    let mut app = loaded(3, 10);

    let effects = app.update(Msg::CommentWritten {
        task_gid: "t0".into(),
        text: Some("Hi".into()),
    });

    assert_eq!(
        effects,
        [Effect::PostComment {
            task_gid: "t0".into(),
            local_gid: "local-1".into(),
            text: "Hi".into()
        }]
    );
}
