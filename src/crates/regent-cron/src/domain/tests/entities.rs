//! Schedule parsing (am/pm, 24h, `once`) and local-time `next_after`.

use super::*;

#[test]
fn parses_supported_formats_and_rejects_garbage() {
    assert_eq!(
        Schedule::parse("30m").unwrap(),
        Schedule::Every { seconds: 1_800 }
    );
    assert_eq!(
        Schedule::parse("2h").unwrap(),
        Schedule::Every { seconds: 7_200 }
    );
    assert_eq!(
        Schedule::parse("1d").unwrap(),
        Schedule::Every { seconds: 86_400 }
    );
    assert_eq!(
        Schedule::parse("daily 09:30").unwrap(),
        Schedule::Daily {
            hour: 9,
            minute: 30
        }
    );
    assert_eq!(
        Schedule::parse("@1000.5").unwrap(),
        Schedule::OneShot { at_epoch: 1000.5 }
    );
    for bad in [
        "",
        "0m",
        "5x",
        "daily 25:00",
        "daily nine",
        "@soon",
        "monday",
        "once 25:00",
        "once noon",
    ] {
        assert!(Schedule::parse(bad).is_err(), "should reject {bad}");
    }
}

#[test]
fn parses_am_pm_and_bare_24h_clocks() {
    let daily = |s: &str| match Schedule::parse(s) {
        Ok(Schedule::Daily { hour, minute }) => (hour, minute),
        other => panic!("{s} → {other:?}"),
    };
    assert_eq!(daily("daily 09:30"), (9, 30));
    assert_eq!(daily("daily 9:30 pm"), (21, 30));
    assert_eq!(daily("daily 8pm"), (20, 0));
    assert_eq!(daily("daily 12am"), (0, 0)); // midnight, not noon
    assert_eq!(daily("daily 12pm"), (12, 0)); // noon, not midnight
    assert_eq!(daily("daily 12:30"), (12, 30)); // bare = 24h, stays noon

    // `once` retires after one fire, and lands in the future.
    let Ok(Schedule::OneShot { at_epoch }) = Schedule::parse("once 8pm") else {
        panic!("once should parse to a one-shot");
    };
    assert!(at_epoch > now_epoch());
}

#[test]
fn next_after_semantics() {
    let every = Schedule::Every { seconds: 60 };
    assert_eq!(every.next_after(100.0), Some(160.0));

    // Daily is LOCAL wall time, so assert the property (next instant whose
    // local clock reads 07:10, within a day) rather than a fixed epoch —
    // the old fixed numbers only held in UTC and hid this bug.
    let daily = Schedule::Daily {
        hour: 7,
        minute: 10,
    };
    for now in [0.0, 700.0, 1_700_000_000.0] {
        let at = daily.next_after(now).expect("daily always has a next");
        assert!(at > now && at <= now + 86_400.0, "within a day of {now}");
        let local = chrono::Local.timestamp_opt(at as i64, 0).unwrap();
        use chrono::Timelike;
        assert_eq!((local.hour(), local.minute()), (7, 10), "local 07:10");
    }

    let oneshot = Schedule::OneShot { at_epoch: 500.0 };
    assert_eq!(oneshot.next_after(100.0), Some(500.0));
    assert_eq!(oneshot.next_after(600.0), None);
}
