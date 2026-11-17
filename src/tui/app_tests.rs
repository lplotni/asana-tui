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
            // Newest first, one minute apart.
            n.created_at = crate::present::format::timestamp(1_794_834_000 - 60 * i as i64);
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
fn r_and_the_timer_refresh_only_the_newest_notifications() {
    let mut app = loaded(150, 10);

    assert_eq!(
        keys(&mut app, "R"),
        [Effect::LoadInbox {
            target: REFRESH_WINDOW
        }]
    );
    app.update(Msg::InboxLoaded(Ok((inbox(150, |_| false), None))));
    assert_eq!(
        app.update(Msg::Tick),
        [Effect::LoadInbox {
            target: REFRESH_WINDOW
        }]
    );
}

#[test]
fn a_refresh_merges_with_the_older_notifications_already_loaded() {
    let mut app = loaded_with(inbox(50, |_| false), Some("old-offset"), 10);
    keys(&mut app, "R");
    // The newest 30 again: one new notification on top, "5" archived on the web.
    let mut fresh: Vec<Notification> = inbox(30, |_| false)
        .into_iter()
        .filter(|n| n.gid != "5")
        .collect();
    let mut newest = fresh[0].clone();
    newest.gid = "new".into();
    newest.created_at = crate::present::format::timestamp(1_794_834_000 + 60);
    fresh.insert(0, newest);

    app.update(Msg::InboxLoaded(Ok((fresh, Some("fresh-offset".into())))));

    let gids = visible_gids(&app);
    assert_eq!(
        gids.len(),
        1 + 29 + 20,
        "new + refreshed 0..29 without 5 + kept 30..49"
    );
    assert_eq!(gids[0], "new");
    assert!(!gids.contains(&"5"));
    assert_eq!(gids.last(), Some(&"49"));
    assert_eq!(
        keys(&mut app, "G"),
        [Effect::LoadMore {
            offset: "old-offset".into()
        }],
        "more loads continue after the kept older items"
    );
}

#[test]
fn a_refresh_that_reaches_the_end_replaces_the_list() {
    let mut app = loaded(50, 10);
    keys(&mut app, "R");

    app.update(Msg::InboxLoaded(Ok((inbox(10, |_| false), None))));

    assert_eq!(app.visible_len(), 10);
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
        attachments: Vec::new(),
        comments_error: None,
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
    load_task(app, gid, task_detail());
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
    let total = crate::present::detail::lines(&task_detail(), 60, app.now).len();

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

/// What the editor overlay is editing: (kind, task, text).
fn editing(app: &App) -> Option<(EditorKind, &str, String)> {
    app.editor
        .as_ref()
        .map(|e| (e.kind, e.task_gid.as_str(), e.text()))
}

fn ctrl(app: &mut App, c: char) -> Vec<Effect> {
    key(app, KeyCode::Char(c), KeyModifiers::CONTROL)
}

#[test]
fn c_and_tab_c_open_the_editor_for_the_selected_task() {
    let mut app = loaded(3, 10);

    assert!(keys(&mut app, "c").is_empty());
    assert_eq!(
        editing(&app),
        Some((EditorKind::Comment, "t0", String::new()))
    );
    key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert!(keys(&mut app, "c").is_empty());
    assert_eq!(
        editing(&app),
        Some((EditorKind::Comment, "t0", String::new()))
    );
}

#[test]
fn keys_typed_in_the_editor_are_text_not_commands() {
    let mut app = loaded(3, 10);
    keys(&mut app, "c");

    let effects = keys(&mut app, "qjx");

    assert!(effects.is_empty());
    assert!(!app.should_quit);
    assert_eq!(editing(&app).unwrap().2, "qjx");
}

#[test]
fn enter_starts_a_new_line_in_the_editor() {
    let mut app = loaded(3, 10);
    keys(&mut app, "c");

    keys(&mut app, "a");
    key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    keys(&mut app, "b");

    assert_eq!(editing(&app).unwrap().2, "a\nb");
}

#[test]
fn ctrl_s_posts_the_comment_and_closes_the_editor() {
    let mut app = loaded(3, 10);
    keys(&mut app, "c");
    keys(&mut app, "Hi");

    let effects = ctrl(&mut app, 's');

    assert_eq!(
        effects,
        [Effect::PostComment {
            task_gid: "t0".into(),
            local_gid: "local-1".into(),
            text: "Hi".into()
        }]
    );
    assert!(app.editor.is_none());
}

#[test]
fn an_empty_comment_is_not_posted() {
    let mut app = loaded(3, 10);
    keys(&mut app, "c");
    keys(&mut app, "  ");

    let effects = ctrl(&mut app, 's');

    assert!(effects.is_empty());
    assert!(app.editor.is_none());
    assert_eq!(
        app.toast.as_ref().unwrap().text,
        "Empty comment; nothing posted"
    );
}

#[test]
fn cancelling_a_comment_keeps_it_as_a_draft() {
    for cancel in [KeyCode::Esc, KeyCode::Char('c')] {
        let mut app = loaded(3, 10);
        keys(&mut app, "c");
        keys(&mut app, "Hi");

        let modifiers = if cancel == KeyCode::Esc {
            KeyModifiers::NONE
        } else {
            KeyModifiers::CONTROL
        };
        let effects = key(&mut app, cancel, modifiers);

        assert!(effects.is_empty());
        assert!(app.editor.is_none());
        assert!(!app.should_quit, "Ctrl-c in the editor cancels, not quits");
        keys(&mut app, "c");
        assert_eq!(editing(&app).unwrap().2, "Hi");
    }
}

#[test]
fn ctrl_o_hands_the_comment_to_the_external_editor() {
    let mut app = loaded(3, 10);
    keys(&mut app, "c");
    keys(&mut app, "Hi");

    let effects = ctrl(&mut app, 'o');

    assert_eq!(
        effects,
        [Effect::EditExternal {
            kind: EditorKind::Comment,
            task_gid: "t0".into(),
            initial: "Hi".into()
        }]
    );
    assert!(app.editor.is_none());
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

    let effects = app.update(Msg::Written {
        kind: EditorKind::Comment,
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

    let effects = app.update(Msg::Written {
        kind: EditorKind::Comment,
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
    app.update(Msg::Written {
        kind: EditorKind::Comment,
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
    app.update(Msg::Written {
        kind: EditorKind::Comment,
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
    keys(&mut app, "c");
    assert_eq!(
        editing(&app),
        Some((EditorKind::Comment, "t0", "Ship it\n".into()))
    );
}

#[test]
fn commenting_works_before_the_details_have_loaded() {
    let mut app = loaded(3, 10);

    let effects = app.update(Msg::Written {
        kind: EditorKind::Comment,
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

// --- Search (`/`) and command line (`:`) -----------------------------------

/// Notifications "0".."4" about tasks named apple, banana, cherry, apricot, date.
fn fruit_inbox() -> App {
    let mut notifications = inbox(5, |_| false);
    for (n, name) in notifications
        .iter_mut()
        .zip(["apple", "banana", "cherry", "apricot", "date"])
    {
        n.work_objects[0].name = name.into();
    }
    loaded_with(notifications, None, 10)
}

fn enter(app: &mut App) -> Vec<Effect> {
    key(app, KeyCode::Enter, KeyModifiers::NONE)
}

fn esc(app: &mut App) -> Vec<Effect> {
    key(app, KeyCode::Esc, KeyModifiers::NONE)
}

#[test]
fn slash_filters_the_list_as_you_type() {
    let mut app = fruit_inbox();

    keys(&mut app, "/ap");

    assert_eq!(visible_gids(&app), ["0", "3"]);
    assert_eq!(app.search.as_ref().unwrap().query, "ap");
}

#[test]
fn search_matches_actor_and_action_too_ignoring_case() {
    let mut app = fruit_inbox();
    let actor = app.notifications[0]
        .created_by
        .as_ref()
        .unwrap()
        .name
        .to_uppercase();

    keys(&mut app, &format!("/{actor}"));

    assert_eq!(visible_gids(&app).len(), 5);
}

#[test]
fn letters_typed_in_search_are_not_commands() {
    let mut app = fruit_inbox();

    keys(&mut app, "/qe");

    assert!(!app.should_quit);
    assert!(app.notifications.iter().all(|n| !n.is_read));
}

#[test]
fn enter_keeps_the_filter_and_esc_then_clears_it_on_the_same_item() {
    let mut app = fruit_inbox();
    keys(&mut app, "/ap");
    enter(&mut app);
    keys(&mut app, "j");
    assert_eq!(selected_gid(&app), "3");
    assert!(!app.search.as_ref().unwrap().editing);

    esc(&mut app);

    assert_eq!(app.search, None);
    assert_eq!(visible_gids(&app).len(), 5);
    assert_eq!(selected_gid(&app), "3");
}

#[test]
fn esc_while_typing_cancels_the_search() {
    let mut app = fruit_inbox();
    keys(&mut app, "/ch");

    esc(&mut app);

    assert_eq!(app.search, None);
    assert_eq!(visible_gids(&app).len(), 5);
}

#[test]
fn n_and_capital_n_move_between_matches_and_wrap_around() {
    let mut app = fruit_inbox();
    keys(&mut app, "/ap");
    enter(&mut app);
    assert_eq!(visible_gids(&app), ["0", "3"]);

    keys(&mut app, "n");
    assert_eq!(selected_gid(&app), "3");
    keys(&mut app, "n");
    assert_eq!(selected_gid(&app), "0", "wraps to the first match");
    keys(&mut app, "N");
    assert_eq!(selected_gid(&app), "3", "wraps back to the last");
}

#[test]
fn colon_q_quits() {
    let mut app = fruit_inbox();

    keys(&mut app, ":q");
    enter(&mut app);

    assert!(app.should_quit);
}

#[test]
fn colon_refresh_reloads() {
    let mut app = fruit_inbox();

    keys(&mut app, ":refresh");

    assert_eq!(
        enter(&mut app),
        [Effect::LoadInbox {
            target: INITIAL_LOAD
        }]
    );
}

#[test]
fn colon_read_all_marks_every_loaded_unread_item_in_one_batch() {
    let mut app = loaded_with(inbox(4, |i| i == 2), None, 10);

    keys(&mut app, ":read-all");
    let effects = enter(&mut app);

    assert_eq!(
        effects,
        [Effect::MarkRead {
            gids: vec!["0".into(), "1".into(), "3".into()],
            read: true
        }]
    );
    assert!(app.toast.as_ref().unwrap().text.contains('3'));
}

#[test]
fn colon_unread_toggles_the_unread_only_filter() {
    let mut app = fruit_inbox();

    keys(&mut app, ":unread");
    enter(&mut app);

    assert!(app.unread_only);
}

#[test]
fn an_unknown_command_says_so_and_changes_nothing() {
    let mut app = fruit_inbox();

    keys(&mut app, ":foo");
    let effects = enter(&mut app);

    assert!(effects.is_empty());
    assert_eq!(app.toast.as_ref().unwrap().text, "Unknown command: foo");
    assert!(app.toast.as_ref().unwrap().is_error);
    assert_eq!(app.command, None);
}

#[test]
fn esc_closes_the_command_line_without_running_it() {
    let mut app = fruit_inbox();
    keys(&mut app, ":q");

    esc(&mut app);

    assert!(!app.should_quit);
    assert_eq!(app.command, None);
}

// --- Help -------------------------------------------------------------------

#[test]
fn question_mark_opens_the_help_and_the_next_key_only_closes_it() {
    let mut app = loaded(3, 10);

    keys(&mut app, "?");
    assert!(app.help);

    keys(&mut app, "j");
    assert!(!app.help);
    assert_eq!(app.cursor, 0, "the closing key did nothing else");
}

// --- Description (T11) -------------------------------------------------------

fn with_notes(app: &mut App, notes: &str, html: &str) {
    let mut detail = task_detail();
    detail.task.notes = notes.into();
    detail.task.html_notes = Some(html.into());
    detail.task.modified_at = Some("2026-11-16T10:00:00.000Z".into());
    app.update(Msg::DetailDue { gid: "t0".into() });
    app.update(Msg::DetailLoaded {
        gid: "t0".into(),
        result: Ok(Box::new(detail)),
    });
}

fn tab_e(app: &mut App) -> Vec<Effect> {
    key(app, KeyCode::Tab, KeyModifiers::NONE);
    keys(app, "e")
}

fn notes_of(app: &App) -> String {
    match app.selected_detail() {
        Some(Detail::Loaded(d)) => d.task.notes.clone(),
        _ => panic!("not loaded"),
    }
}

#[test]
fn tab_e_opens_the_editor_with_the_plain_description() {
    let mut app = loaded(3, 10);
    with_notes(&mut app, "Old text", "<body>Old text</body>");

    assert!(tab_e(&mut app).is_empty());
    assert_eq!(
        editing(&app),
        Some((EditorKind::Description, "t0", "Old text".into()))
    );
}

#[test]
fn ctrl_s_saves_the_edited_description() {
    let mut app = loaded(3, 10);
    with_notes(&mut app, "Old text", "<body>Old text</body>");
    tab_e(&mut app);
    keys(&mut app, "!");

    let effects = ctrl(&mut app, 's');

    assert!(
        matches!(&effects[..], [Effect::SaveNotes { task_gid, text, .. }] if task_gid == "t0" && text == "!Old text"),
        "{effects:?}"
    );
    assert!(app.editor.is_none());
    assert_eq!(notes_of(&app), "!Old text");
}

#[test]
fn cancelling_a_description_edit_discards_it() {
    let mut app = loaded(3, 10);
    with_notes(&mut app, "Old text", "<body>Old text</body>");
    tab_e(&mut app);
    keys(&mut app, "!");

    key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    tab_e(&mut app);

    assert_eq!(editing(&app).unwrap().2, "Old text");
}

#[test]
fn ctrl_o_hands_the_description_to_the_external_editor() {
    let mut app = loaded(3, 10);
    with_notes(&mut app, "Old text", "<body>Old text</body>");
    tab_e(&mut app);

    let effects = ctrl(&mut app, 'o');

    assert_eq!(
        effects,
        [Effect::EditExternal {
            kind: EditorKind::Description,
            task_gid: "t0".into(),
            initial: "Old text".into()
        }]
    );
}

#[test]
fn tab_e_waits_for_the_task_to_load() {
    let mut app = loaded(3, 10);

    assert!(tab_e(&mut app).is_empty());
    assert!(app.toast.is_some());
}

#[test]
fn a_formatted_description_asks_before_editing() {
    let mut app = loaded(3, 10);
    with_notes(
        &mut app,
        "Old text",
        "<body><strong>Old</strong> text</body>",
    );

    assert!(tab_e(&mut app).is_empty());
    assert!(app.confirm.is_some());

    assert!(keys(&mut app, "n").is_empty());
    assert!(app.confirm.is_none());

    tab_e(&mut app);
    assert!(keys(&mut app, "y").is_empty());
    assert_eq!(
        editing(&app),
        Some((EditorKind::Description, "t0", "Old text".into()))
    );
}

#[test]
fn an_unchanged_description_is_not_saved() {
    let mut app = loaded(3, 10);
    with_notes(&mut app, "Old text", "<body>Old text</body>");

    let effects = app.update(Msg::Written {
        kind: EditorKind::Description,
        task_gid: "t0".into(),
        text: Some("Old text\n".into()),
    });

    assert!(effects.is_empty());
    assert_eq!(app.toast.as_ref().unwrap().text, "No changes");
}

#[test]
fn a_changed_description_shows_at_once_and_is_saved_with_a_conflict_check() {
    let mut app = loaded(3, 10);
    with_notes(&mut app, "Old text", "<body>Old text</body>");

    let effects = app.update(Msg::Written {
        kind: EditorKind::Description,
        task_gid: "t0".into(),
        text: Some("New text\n".into()),
    });

    assert_eq!(
        effects,
        [Effect::SaveNotes {
            task_gid: "t0".into(),
            text: "New text\n".into(),
            expected_modified_at: Some("2026-11-16T10:00:00.000Z".into()),
        }]
    );
    assert_eq!(notes_of(&app), "New text\n");
}

#[test]
fn a_saved_description_reloads_the_task() {
    let mut app = loaded(3, 10);
    with_notes(&mut app, "Old text", "<body>Old text</body>");
    app.update(Msg::Written {
        kind: EditorKind::Description,
        task_gid: "t0".into(),
        text: Some("New text\n".into()),
    });

    let effects = app.update(Msg::NotesSaved {
        task_gid: "t0".into(),
        result: Ok(()),
    });

    assert_eq!(effects, [Effect::LoadDetail { gid: "t0".into() }]);
    assert!(!app.toast.as_ref().unwrap().is_error);
}

#[test]
fn a_failed_save_restores_the_old_text_and_says_where_the_new_one_is() {
    let mut app = loaded(3, 10);
    with_notes(&mut app, "Old text", "<body>Old text</body>");
    app.update(Msg::Written {
        kind: EditorKind::Description,
        task_gid: "t0".into(),
        text: Some("New text\n".into()),
    });

    app.update(Msg::NotesSaved {
        task_gid: "t0".into(),
        result: Err(SaveFailure {
            message: "the task was changed on the web while you were editing".into(),
            kept_at: Some("/tmp/asn-description-t0-1.md".into()),
        }),
    });

    assert_eq!(notes_of(&app), "Old text");
    let toast = app.toast.as_ref().unwrap();
    assert!(toast.is_error);
    assert!(
        toast.text.contains("/tmp/asn-description-t0-1.md"),
        "{}",
        toast.text
    );
}

// --- Commenting marks the task's notifications read ---------------------------

#[test]
fn a_posted_comment_marks_the_tasks_unread_notifications_read() {
    // Notifications "0" and "2" are about task t0; "1" is about t1.
    let mut notifications = inbox(3, |_| false);
    notifications[2].work_objects[0].gid = "t0".into();
    let mut app = loaded_with(notifications, None, 10);
    loaded_detail(&mut app, "t0");
    app.update(Msg::Written {
        kind: EditorKind::Comment,
        task_gid: "t0".into(),
        text: Some("Done".into()),
    });
    let mut stored = task_detail().comments[0].clone();
    stored.gid = "999".into();

    let effects = app.update(Msg::CommentPosted {
        task_gid: "t0".into(),
        local_gid: "local-1".into(),
        result: Ok(Box::new(stored)),
    });

    assert_eq!(
        effects,
        [Effect::MarkRead {
            gids: vec!["0".into(), "2".into()],
            read: true
        }]
    );
    let read: Vec<bool> = app.notifications.iter().map(|n| n.is_read).collect();
    assert_eq!(read, [true, false, true]);
}

#[test]
fn a_failed_comment_leaves_the_notifications_unread() {
    let mut app = loaded(3, 10);
    app.update(Msg::Written {
        kind: EditorKind::Comment,
        task_gid: "t0".into(),
        text: Some("Done".into()),
    });

    let effects = app.update(Msg::CommentPosted {
        task_gid: "t0".into(),
        local_gid: "local-1".into(),
        result: Err("timed out".into()),
    });

    assert!(effects.is_empty());
    assert!(!app.notifications[0].is_read);
}

#[test]
fn commenting_on_an_already_read_task_sends_no_mark() {
    let mut app = loaded_with(inbox(2, |_| true), None, 10);
    app.update(Msg::Written {
        kind: EditorKind::Comment,
        task_gid: "t0".into(),
        text: Some("Done".into()),
    });
    let mut stored = task_detail().comments[0].clone();
    stored.gid = "999".into();

    let effects = app.update(Msg::CommentPosted {
        task_gid: "t0".into(),
        local_gid: "local-1".into(),
        result: Ok(Box::new(stored)),
    });

    assert!(effects.is_empty());
}

// --- Images ------------------------------------------------------------------

const IMAGE: &str = "1200000000000301";

fn task_with_attachments() -> TaskDetail {
    let text = std::fs::read_to_string("tests/fixtures/attachments.json").unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    TaskDetail {
        attachments: serde_json::from_value(value["data"].clone()).unwrap(),
        ..task_detail()
    }
}

fn load_task(app: &mut App, gid: &str, detail: TaskDetail) -> Vec<Effect> {
    app.update(Msg::DetailDue { gid: gid.into() });
    app.update(Msg::DetailLoaded {
        gid: gid.into(),
        result: Ok(Box::new(detail)),
    })
}

fn image_loads(effects: &[Effect]) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::LoadImage { attachment_gid, .. } => Some(attachment_gid.as_str()),
            _ => None,
        })
        .collect()
}

fn tiny_image() -> Arc<image::DynamicImage> {
    Arc::new(image::DynamicImage::new_rgba8(2, 2))
}

#[test]
fn a_loaded_task_downloads_only_its_images() {
    let mut app = loaded(3, 10);

    let effects = load_task(&mut app, "t0", task_with_attachments());

    let url = task_with_attachments().attachments[0]
        .download_url
        .clone()
        .unwrap();
    assert_eq!(
        effects,
        [Effect::LoadImage {
            attachment_gid: IMAGE.into(),
            url
        }]
    );
    assert!(matches!(app.images.get(IMAGE), Some(ImageState::Loading)));
}

#[test]
fn only_https_image_links_are_downloaded() {
    let mut app = loaded(3, 10);
    let mut detail = task_with_attachments();
    detail.attachments[0].download_url = Some("http://example.com/a.png".into());

    let effects = load_task(&mut app, "t0", detail);

    assert!(image_loads(&effects).is_empty());
}

#[test]
fn a_downloaded_image_is_kept() {
    let mut app = loaded(3, 10);
    load_task(&mut app, "t0", task_with_attachments());

    app.update(Msg::ImageLoaded {
        attachment_gid: IMAGE.into(),
        result: Ok(tiny_image()),
    });

    assert!(matches!(app.images.get(IMAGE), Some(ImageState::Loaded(_))));
}

#[test]
fn a_failed_image_is_retried_when_the_task_is_reloaded() {
    let mut app = loaded(3, 10);
    load_task(&mut app, "t0", task_with_attachments());
    app.update(Msg::ImageLoaded {
        attachment_gid: IMAGE.into(),
        result: Err("403".into()),
    });
    assert!(matches!(app.images.get(IMAGE), Some(ImageState::Failed(_))));

    let effects = app.update(Msg::DetailLoaded {
        gid: "t0".into(),
        result: Ok(Box::new(task_with_attachments())),
    });

    assert_eq!(image_loads(&effects), [IMAGE]);
}

#[test]
fn an_image_already_loaded_is_not_downloaded_again() {
    let mut app = loaded(3, 10);
    load_task(&mut app, "t0", task_with_attachments());
    app.update(Msg::ImageLoaded {
        attachment_gid: IMAGE.into(),
        result: Ok(tiny_image()),
    });

    let effects = app.update(Msg::DetailLoaded {
        gid: "t0".into(),
        result: Ok(Box::new(task_with_attachments())),
    });

    assert!(image_loads(&effects).is_empty());
}

#[test]
fn only_the_newest_images_are_kept_in_memory() {
    let mut app = loaded(3, 10);
    for i in 0..=MAX_IMAGES {
        app.update(Msg::ImageLoaded {
            attachment_gid: format!("img{i}"),
            result: Ok(tiny_image()),
        });
    }

    assert_eq!(app.images.len(), MAX_IMAGES);
    assert!(!app.images.contains_key("img0"));
    assert!(app.images.contains_key(&format!("img{MAX_IMAGES}")));
}

#[test]
fn returning_to_a_cached_task_reloads_its_forgotten_images() {
    let mut app = loaded(3, 10);
    load_task(&mut app, "t0", task_with_attachments());
    keys(&mut app, "j");
    app.images.clear();

    let effects = keys_all(&mut app, "k");

    assert_eq!(image_loads(&effects), [IMAGE]);
}

#[test]
fn the_detail_pane_scrolls_past_the_image_space() {
    let mut app = loaded(3, 10);
    load_task(&mut app, "t0", task_with_attachments());

    let (lines, _) = detail::lines_with_images(&task_with_attachments(), 60, app.now, IMAGE_ROWS);
    assert_eq!(app.detail_max_scroll(), lines.len() - 20);
}

// --- Workspace task search (`S`) ---------------------------------------------

fn found(gid: &str, name: &str, completed: bool) -> crate::api::search::FoundTask {
    crate::api::search::FoundTask {
        gid: gid.into(),
        name: name.into(),
        completed,
    }
}

/// Opens task search, types `query` and delivers `results` for it.
fn searched(app: &mut App, query: &str, results: Vec<crate::api::search::FoundTask>) {
    keys(app, "S");
    keys(app, query);
    let seq = app.picker.as_ref().unwrap().search_seq;
    app.update(Msg::SearchDue { seq });
    app.update(Msg::TasksFound {
        seq,
        result: Ok(results),
    });
}

#[test]
fn capital_s_opens_an_empty_task_search() {
    let mut app = loaded(3, 10);

    let effects = keys(&mut app, "S");

    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.stage, picker::Stage::Task);
    assert_eq!(picker.query, "");
    assert!(!picker.loading);
    assert!(effects.is_empty());
}

#[test]
fn typing_searches_the_workspace_after_a_pause() {
    let mut app = loaded(3, 10);
    keys(&mut app, "S");

    let typed = keys(&mut app, "ab");
    let seq = app.picker.as_ref().unwrap().search_seq;
    let due = app.update(Msg::SearchDue { seq });

    assert!(
        typed
            .iter()
            .all(|e| matches!(e, Effect::ScheduleSearch { .. }))
    );
    assert_eq!(
        due,
        [Effect::SearchTasks {
            query: "ab".into(),
            seq
        }]
    );
}

#[test]
fn a_blank_query_does_not_search() {
    let mut app = loaded(3, 10);
    keys(&mut app, "S");
    keys(&mut app, " ");
    let seq = app.picker.as_ref().unwrap().search_seq;

    let due = app.update(Msg::SearchDue { seq });

    assert!(due.is_empty());
    assert!(!app.picker.as_ref().unwrap().loading);
}

#[test]
fn found_tasks_are_listed_and_completed_ones_marked() {
    let mut app = loaded(3, 10);

    searched(
        &mut app,
        "check",
        vec![
            found("11", "Open one", false),
            found("12", "Done one", true),
        ],
    );

    let items: Vec<(&str, bool)> = app
        .picker
        .as_ref()
        .unwrap()
        .items
        .iter()
        .map(|i| (i.name.as_str(), i.checked))
        .collect();
    assert_eq!(items, [("Open one", false), ("Done one", true)]);
}

#[test]
fn results_of_an_older_query_are_ignored() {
    let mut app = loaded(3, 10);
    keys(&mut app, "S");
    keys(&mut app, "a");
    let old = app.picker.as_ref().unwrap().search_seq;
    keys(&mut app, "b");

    app.update(Msg::TasksFound {
        seq: old,
        result: Ok(vec![found("11", "Stale", false)]),
    });

    assert!(app.picker.as_ref().unwrap().items.is_empty());
}

#[test]
fn a_failed_task_search_says_why() {
    let mut app = loaded(3, 10);
    keys(&mut app, "S");
    keys(&mut app, "a");
    let seq = app.picker.as_ref().unwrap().search_seq;

    app.update(Msg::TasksFound {
        seq,
        result: Err("402 premium only".into()),
    });

    assert_eq!(
        app.toast.as_ref().unwrap().text,
        "Task search failed: 402 premium only"
    );
    assert!(app.toast.as_ref().unwrap().is_error);
}

#[test]
fn choosing_a_found_task_shows_it_in_the_task_pane() {
    let mut app = loaded(3, 10);
    searched(&mut app, "check", vec![found("11", "Open one", false)]);

    let effects = key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(app.picker.is_none());
    assert_eq!(app.focus, Focus::Detail);
    assert_eq!(app.selected_task_gid(), Some("11"));
    assert_eq!(app.shown_task_name(), Some("Open one"));
    assert_eq!(effects, [Effect::LoadDetail { gid: "11".into() }]);
    assert_eq!(app.selected_detail(), Some(&Detail::Loading));
}

#[test]
fn choosing_a_task_already_loaded_does_not_load_it_again() {
    let mut app = loaded(3, 10);
    loaded_detail(&mut app, "t2");
    searched(&mut app, "check", vec![found("t2", "Cached", false)]);

    let effects = key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::LoadDetail { .. }))
    );
    assert!(matches!(app.selected_detail(), Some(Detail::Loaded(_))));
}

#[test]
fn leaving_the_task_pane_returns_to_the_inbox_selection() {
    for leave in ["h", "q"] {
        let mut app = loaded(3, 10);
        searched(&mut app, "check", vec![found("11", "Open one", false)]);
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

        keys(&mut app, leave);

        assert_eq!(app.focus, Focus::List, "{leave}");
        assert_eq!(app.selected_task_gid(), Some("t0"), "{leave}");
    }
}

#[test]
fn commenting_on_a_found_task_comments_on_that_task() {
    let mut app = loaded(3, 10);
    searched(&mut app, "check", vec![found("11", "Open one", false)]);
    key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    keys(&mut app, "c");

    assert_eq!(
        editing(&app),
        Some((EditorKind::Comment, "11", String::new()))
    );
}

#[test]
fn a_found_task_that_is_still_loading_opens_by_its_gid_in_the_browser() {
    let mut app = loaded(3, 10);
    searched(&mut app, "check", vec![found("11", "Open one", false)]);
    key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    let effects = keys(&mut app, "o");

    assert_eq!(
        effects,
        [Effect::OpenUrl {
            url: "https://app.asana.com/0/0/11".into()
        }]
    );
}

// --- Moving a task between sections (`m`) -------------------------------------

/// Project "1200000000000043" (section "…044") and "1200000000000045"
/// (section "…046"), from `task_full.json`.
const PROJECT: &str = "1200000000000043";
const SECTION: &str = "1200000000000044";

fn named(gid: &str, name: &str) -> Named {
    Named {
        gid: gid.into(),
        name: name.into(),
    }
}

/// The task "t0" loaded, in only its first project when `one_project`.
fn task_in_projects(one_project: bool) -> App {
    let mut app = loaded(3, 10);
    let mut detail = task_detail();
    if one_project {
        detail.task.memberships.truncate(1);
    }
    load_task(&mut app, "t0", detail);
    app
}

fn section_of(app: &App, project: &str) -> Option<String> {
    match app.selected_detail() {
        Some(Detail::Loaded(d)) => d
            .task
            .memberships
            .iter()
            .find(|m| m.project.gid == project)
            .and_then(|m| m.section.as_ref().map(|s| s.name.clone())),
        _ => panic!("not loaded"),
    }
}

/// Opens `m` on a one-project task and delivers its sections.
fn choosing_section(app: &mut App) {
    keys(app, "m");
    app.update(Msg::SectionsLoaded {
        project_gid: PROJECT.into(),
        result: Ok(vec![
            named("s-todo", "To do"),
            named(SECTION, "Maple ember"),
            named("s-done", "Done"),
        ]),
    });
}

#[test]
fn m_waits_for_the_task_to_load() {
    let mut app = loaded(3, 10);

    keys(&mut app, "m");

    assert!(app.picker.is_none());
    assert_eq!(
        app.toast.as_ref().unwrap().text,
        "Wait for the task to load"
    );
}

#[test]
fn m_on_a_task_in_no_project_points_to_p() {
    let mut app = loaded(3, 10);
    let mut detail = task_detail();
    detail.task.memberships.clear();
    load_task(&mut app, "t0", detail);

    keys(&mut app, "m");

    assert!(app.picker.is_none());
    assert_eq!(
        app.toast.as_ref().unwrap().text,
        "The task isn't in a project yet; p adds it to one"
    );
}

#[test]
fn m_on_a_task_in_one_project_goes_straight_to_its_sections() {
    let mut app = task_in_projects(true);

    let effects = keys(&mut app, "m");

    assert_eq!(
        effects,
        [Effect::LoadSections {
            project_gid: PROJECT.into()
        }]
    );
    assert!(matches!(
        &app.picker.as_ref().unwrap().stage,
        picker::Stage::MoveSection { project, .. } if project.gid == PROJECT
    ));
}

#[test]
fn m_on_a_task_in_several_projects_asks_which_project_first() {
    let mut app = task_in_projects(false);

    let effects = keys(&mut app, "m");

    assert!(effects.is_empty());
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.stage, picker::Stage::MoveProject);
    let names: Vec<&str> = picker.items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["Stone maple zulu cedar", "Ember zulu"]);

    keys(&mut app, "j");
    let effects = key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(
        effects,
        [Effect::LoadSections {
            project_gid: "1200000000000045".into()
        }]
    );
}

#[test]
fn the_sections_show_where_the_task_is_now() {
    let mut app = task_in_projects(true);

    choosing_section(&mut app);

    let items: Vec<(&str, bool)> = app
        .picker
        .as_ref()
        .unwrap()
        .items
        .iter()
        .map(|i| (i.name.as_str(), i.checked))
        .collect();
    assert_eq!(
        items,
        [("To do", false), ("Maple ember", true), ("Done", false)]
    );
}

#[test]
fn choosing_a_section_moves_the_task_right_away() {
    let mut app = task_in_projects(true);
    choosing_section(&mut app);

    keys(&mut app, "jj");
    let effects = key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(
        effects,
        [Effect::MoveToSection {
            task_gid: "t0".into(),
            project_gid: PROJECT.into(),
            section: named("s-done", "Done"),
            previous: Some(named(SECTION, "Maple ember")),
        }]
    );
    assert!(app.picker.is_none());
    assert_eq!(section_of(&app, PROJECT).as_deref(), Some("Done"));
}

#[test]
fn choosing_the_current_section_changes_nothing() {
    let mut app = task_in_projects(true);
    choosing_section(&mut app);

    keys(&mut app, "j");
    let effects = key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(effects.is_empty());
    assert!(app.picker.is_none());
    assert_eq!(app.toast.as_ref().unwrap().text, "Already in Maple ember");
}

#[test]
fn a_confirmed_move_says_where_the_task_went() {
    let mut app = task_in_projects(true);

    app.update(Msg::MovedToSection {
        task_gid: "t0".into(),
        project_gid: PROJECT.into(),
        section: named("s-done", "Done"),
        previous: Some(named(SECTION, "Maple ember")),
        result: Ok(()),
    });

    assert_eq!(app.toast.as_ref().unwrap().text, "Moved to Done");
    assert!(!app.toast.as_ref().unwrap().is_error);
}

#[test]
fn a_refused_move_puts_the_task_back() {
    let mut app = task_in_projects(true);
    choosing_section(&mut app);
    keys(&mut app, "jj");
    key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    app.update(Msg::MovedToSection {
        task_gid: "t0".into(),
        project_gid: PROJECT.into(),
        section: named("s-done", "Done"),
        previous: Some(named(SECTION, "Maple ember")),
        result: Err("forbidden".into()),
    });

    assert_eq!(section_of(&app, PROJECT).as_deref(), Some("Maple ember"));
    assert_eq!(app.toast.as_ref().unwrap().text, "Not moved: forbidden");
    assert!(app.toast.as_ref().unwrap().is_error);
}

#[test]
fn a_blank_comment_from_the_external_editor_is_not_posted() {
    let mut app = loaded(3, 10);

    let effects = app.update(Msg::Written {
        kind: EditorKind::Comment,
        task_gid: "t0".into(),
        text: Some("  \n".into()),
    });

    assert!(effects.is_empty());
    assert_eq!(
        app.toast.as_ref().unwrap().text,
        "Empty comment; nothing posted"
    );
}
