use super::*;
use chrono::{FixedOffset, Utc};

const LINK: &str = "|cffa335ee|Hitem:16805::::::::60:::::::|h[Felheart Gloves]|h|r";
const PREFIX: &str = "10/13 10:01:51.269  |Hchannel:RAID_LEADER|h[Raid Leader]|h Rae Quill: ";
// 2026-10-13 10:01:51 UTC.
const AT: i64 = 1_791_885_711;

fn raids() -> Vec<KnownRaid> {
    vec![
        KnownRaid {
            id: "cm9xr2k4a0001".into(),
            env: Env::Staging,
            bosses: vec![
                "lb_lucifron".into(),
                "lb_magmadar".into(),
                "lb_trash".into(),
            ],
        },
        KnownRaid {
            id: "cm9zz7q1b0042".into(),
            env: Env::Production,
            bosses: vec!["lb_onyxia".into(), "xx_onyxia".into()],
        },
    ]
}

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 13).unwrap()
}

fn parse(line: &str) -> LineResult {
    parse_line(line, today(), &Utc, &raids())
}

/// A raid leader's chat log line carrying `text`.
fn chat(text: &str) -> String {
    format!("{PREFIX}{text}")
}

fn award_line(machine: &str) -> String {
    chat(&format!("{LINK} goes to Zoé Brûlé (HR, roll 91) {machine}"))
}

fn malformed(line: &str) -> Malformed {
    match parse(line) {
        LineResult::Malformed(m) => m,
        other => panic!("expected malformed, got {other:?}"),
    }
}

fn spec_award(practice: bool) -> Award {
    Award {
        id: "aw-9d2f6h1j".into(),
        raid: "cm9xr2k4a0001".into(),
        env: Env::Staging,
        practice,
        boss: "lb_lucifron".into(),
        item: 16805,
        char: Some("Zoé Brûlé".into()),
        method: LootMethod::Hr,
        roll: Some(91),
        sender: "Rae Quill".into(),
        at: AT,
    }
}

// ADDON-SPEC 9.7 worked examples.

#[test]
fn spec_award_line() {
    let line = award_line("<BF1;A;SA0001;aw-9d2f6h1j;16805;ifron;HR;91;Zoé Brûlé>");
    assert_eq!(
        parse(&line),
        LineResult::Operation(Operation::Award(spec_award(false)))
    );
}

#[test]
fn spec_practice_award_line() {
    let line = chat(&format!(
        "[Practice] {LINK} goes to Zoé Brûlé (HR, roll 91) <BF1P;A;SA0001;aw-9d2f6h1j;16805;ifron;HR;91;Zoé Brûlé>"
    ));
    assert_eq!(
        parse(&line),
        LineResult::Operation(Operation::Award(spec_award(true)))
    );
}

#[test]
fn spec_undo_line() {
    let line = chat(&format!(
        "Undo: {LINK} no longer goes to Zoé Brûlé <BF1;V;SA0001;vd-2s9t4w7y;aw-9d2f6h1j>"
    ));
    assert_eq!(
        parse(&line),
        LineResult::Operation(Operation::Void(Void {
            id: "vd-2s9t4w7y".into(),
            raid: "cm9xr2k4a0001".into(),
            env: Env::Staging,
            practice: false,
            key: "aw-9d2f6h1j".into(),
            at: AT,
        }))
    );
}

#[test]
fn spec_replacement_line() {
    let line = chat(&format!(
        "Correction: {LINK} goes to Bo Marsh (SR, roll 77), not Zoé Brûlé <BF1;R;SA0001;rp-3n7b5x0c;aw-9d2f6h1j;aw-6k1m4v8z;16805;ifron;SR;77;Bo Marsh>"
    ));
    let award = Award {
        id: "aw-6k1m4v8z".into(),
        char: Some("Bo Marsh".into()),
        method: LootMethod::Sr,
        roll: Some(77),
        ..spec_award(false)
    };
    assert_eq!(
        parse(&line),
        LineResult::Operation(Operation::Replace(Replace {
            id: "rp-3n7b5x0c".into(),
            raid: "cm9xr2k4a0001".into(),
            env: Env::Staging,
            practice: false,
            key: "aw-9d2f6h1j".into(),
            award: Box::new(award),
            at: AT,
        }))
    );
}

#[test]
fn accented_sender_and_raid_channel() {
    let line = format!(
        "10/13 10:01:51.269  |Hchannel:RAID|h[Raid]|h Zoé Brûlé: {LINK} goes to Zoé Brûlé (HR, roll 91) <BF1;A;SA0001;aw-9d2f6h1j;16805;ifron;HR;91;Zoé Brûlé>\r\n"
    );
    let LineResult::Operation(Operation::Award(award)) = parse(&line) else {
        panic!("expected an award");
    };
    assert_eq!(award.sender, "Zoé Brûlé");
}

#[test]
fn disenchant_without_winner_or_roll() {
    let line = award_line("<BF1;A;SA0001;aw-9d2f6h1j;16805;trash;DISENCHANT_BANK;;>");
    let LineResult::Operation(Operation::Award(award)) = parse(&line) else {
        panic!("expected an award");
    };
    assert_eq!(
        (award.char, award.roll, award.boss.as_str()),
        (None, None, "lb_trash")
    );
}

#[test]
fn production_raid() {
    let line = award_line("<BF1;A;PB0042;aw-9d2f6h1j;16805;b_ony;OPEN_ROLL;5;Zoé Brûlé>");
    let LineResult::Operation(Operation::Award(award)) = parse_line(
        &line,
        today(),
        &Utc,
        &[KnownRaid {
            id: "cm9zz7q1b0042".into(),
            env: Env::Production,
            bosses: vec!["lb_ony".into()],
        }],
    ) else {
        panic!("expected an award");
    };
    assert_eq!(
        (award.env, award.boss.as_str()),
        (Env::Production, "lb_ony")
    );
}

// Other chat never yields an operation.

#[test]
fn other_chat_is_not_ours() {
    let machine = "<BF1;A;SA0001;aw-9d2f6h1j;16805;ifron;HR;91;Zoé Brûlé>";
    let lines = [
        format!("10/13 10:01:51.269  |Hchannel:GUILD|h[Guild]|h Rae Quill: {machine}"),
        format!("10/13 10:01:51.269  |Hchannel:OFFICER|h[Officer]|h Rae Quill: {machine}"),
        format!("10/13 10:01:51.269  |Hchannel:PARTY|h[Party]|h Rae Quill: {machine}"),
        format!(
            "10/13 10:01:51.269  |Hchannel:RAID_WARNING|h[Raid Warning]|h Rae Quill: {machine}"
        ),
        format!("10/13 10:01:51.269  |Hchannel:channel:2|h[2. Trade]|h Rae Quill: {machine}"),
        format!("10/13 10:01:51.269  Rae Quill whispers: {machine}"),
        format!("10/13 10:01:51.269  To Rae Quill: {machine}"),
        format!("10/13 10:01:51.269  Rae Quill says: {machine}"),
        format!("10/13 10:01:51.269  Rae Quill yells: {machine}"),
        format!(
            "10/13 10:01:51.269  Rae Quill whispers: |Hchannel:RAID|h[Raid]|h Rae Quill: {machine}"
        ),
        "10/13 10:01:51.269  Holyhek Waifu rolls 75 (1-100)".into(),
        "10/13 10:01:51.269  Shootier Hek receives loot: [Nomad Buckler of Healing].".into(),
        "".into(),
        chat("pull in 10, <BFF> after"),
        chat(&format!("{LINK} anyone need this?")),
    ];
    for line in lines {
        assert_eq!(parse(&line), LineResult::NotOurs, "{line}");
    }
}

// ADDON-SPEC 9.4 rejections.

#[test]
fn rejects_malformed_lines() {
    let award = |fields: &str| award_line(&format!("<BF1;A;{fields}>"));
    let ok = "SA0001;aw-9d2f6h1j;16805;ifron;HR;91;Zoé Brûlé";
    let replace = |fields: &str| {
        chat(&format!(
            "Correction: {LINK} goes to Bo Marsh <BF1;R;SA0001;rp-3n7b5x0c;{fields};16805;ifron;SR;77;Bo Marsh>"
        ))
    };
    let cases = [
        (
            award_line(&format!("<BF1;A;{ok}> <BF1;A;{ok}>")),
            Malformed::SeveralMachineParts,
        ),
        (award_line(&format!("<BF1;A;{ok}")), Malformed::Unclosed),
        (
            award_line(&format!("<BF1;A;{ok}> thanks")),
            Malformed::TextAfterMachinePart,
        ),
        (award_line(&format!("<BF1X;A;{ok}>")), Malformed::Version),
        (award_line(&format!("<BF01;A;{ok}>")), Malformed::Version),
        (award_line(&format!("<BF1;Z;{ok}>")), Malformed::Type),
        (award_line("<BF1>"), Malformed::Type),
        (award(&format!("{ok};extra")), Malformed::FieldCount),
        (
            award("SA0001;aw-9d2f6h1j;16805;ifron;HR;91"),
            Malformed::FieldCount,
        ),
        (
            chat(&format!("Undo: {LINK} <BF1;V;SA0001;vd-2s9t4w7y>")),
            Malformed::FieldCount,
        ),
        (
            award("XA0001;aw-9d2f6h1j;16805;ifron;HR;91;Zoé"),
            Malformed::Raid,
        ),
        (
            award("Sa0001;aw-9d2f6h1j;16805;ifron;HR;91;Zoé"),
            Malformed::Raid,
        ),
        (
            award("SA00011;aw-9d2f6h1j;16805;ifron;HR;91;Zoé"),
            Malformed::Raid,
        ),
        (
            award("SB0042;aw-9d2f6h1j;16805;ifron;HR;91;Zoé"),
            Malformed::UnknownRaid,
        ),
        (
            award("PA0001;aw-9d2f6h1j;16805;ifron;HR;91;Zoé"),
            Malformed::UnknownRaid,
        ),
        (
            award("SA0001;aw-9d2f;16805;ifron;HR;91;Zoé"),
            Malformed::Key,
        ),
        (
            award("SA0001;aw-9d2f6h1j-1234567890123;16805;ifron;HR;91;Zoé"),
            Malformed::Key,
        ),
        (
            award("SA0001;aw.9d2f6h1j;16805;ifron;HR;91;Zoé"),
            Malformed::Key,
        ),
        (
            chat(&format!(
                "Undo: {LINK} <BF1;V;SA0001;vd 2s9t4w7y;aw-9d2f6h1j>"
            )),
            Malformed::Id,
        ),
        (replace("aw-9d2f6h1j;aw-6k1m"), Malformed::NewKey),
        (replace("aw-9d2f6h1j;aw-9d2f6h1j"), Malformed::SameKey),
        (
            award("SA0001;aw-9d2f6h1j;0;ifron;HR;91;Zoé"),
            Malformed::Item,
        ),
        (
            award("SA0001;aw-9d2f6h1j;12345678;ifron;HR;91;Zoé"),
            Malformed::Item,
        ),
        (
            award("SA0001;aw-9d2f6h1j;+16805;ifron;HR;91;Zoé"),
            Malformed::Item,
        ),
        (
            award("SA0001;aw-9d2f6h1j;16805;ifron;HR;0;Zoé"),
            Malformed::Roll,
        ),
        (
            award("SA0001;aw-9d2f6h1j;16805;ifron;HR;101;Zoé"),
            Malformed::Roll,
        ),
        (
            award("SA0001;aw-9d2f6h1j;16805;ifron;HR;high;Zoé"),
            Malformed::Roll,
        ),
        (
            award("SA0001;aw-9d2f6h1j;16805;ifron;hr;91;Zoé"),
            Malformed::Method,
        ),
        (
            award("SA0001;aw-9d2f6h1j;16805;ifron;HR;91;"),
            Malformed::Winner,
        ),
        (
            award("SA0001;aw-9d2f6h1j;16805;ifron;DISENCHANT_BANK;;Zoé"),
            Malformed::Winner,
        ),
        (
            chat(&format!("Zoé gets it <BF1;A;{ok}>")),
            Malformed::NoItemLink,
        ),
        (
            chat("Undo: no longer <BF1;V;SA0001;vd-2s9t4w7y;aw-9d2f6h1j>"),
            Malformed::NoItemLink,
        ),
        (
            award("SA0001;aw-9d2f6h1j;16806;ifron;HR;91;Zoé"),
            Malformed::ItemLinkMismatch,
        ),
        (award_line(&format!("<BF1P;A;{ok}>")), Malformed::Practice),
        (
            chat(&format!("[Practice] {LINK} <BF1;A;{ok}>")),
            Malformed::Practice,
        ),
        (
            award("SA0001;aw-9d2f6h1j;16805;nobos;HR;91;Zoé"),
            Malformed::UnknownBoss,
        ),
        (
            award("SA0001;aw-9d2f6h1j;16805;IFRON;HR;91;Zoé"),
            Malformed::UnknownBoss,
        ),
        (
            award("SA0001;aw-9d2f6h1j;16805;%ZZ;HR;91;Zoé"),
            Malformed::Escape,
        ),
        (
            format!("13/45 10:01:51.269  |Hchannel:RAID|h[Raid]|h Rae Quill: {LINK} <BF1;A;{ok}>"),
            Malformed::Time,
        ),
        (
            format!("10/13 25:01:51.269  |Hchannel:RAID|h[Raid]|h Rae Quill: {LINK} <BF1;A;{ok}>"),
            Malformed::Time,
        ),
    ];
    for (line, want) in cases {
        assert_eq!(malformed(&line), want, "{line}");
    }
}

#[test]
fn boss_code_matching_several_bosses() {
    let line = award_line("<BF1;A;PB0042;aw-9d2f6h1j;16805;onyxia;HR;91;Zoé>");
    assert_eq!(malformed(&line), Malformed::UnknownBoss);
    let line = award_line("<BF1;A;PB0042;aw-9d2f6h1j;16805;nyxia;HR;91;Zoé>");
    assert_eq!(malformed(&line), Malformed::SeveralBosses);
}

#[test]
fn raid_code_matching_several_raids() {
    let mut known = raids();
    known.push(KnownRaid {
        id: "zz0000a0001".into(),
        ..known[0].clone()
    });
    let line = award_line("<BF1;A;SA0001;aw-9d2f6h1j;16805;ifron;HR;91;Zoé>");
    assert_eq!(
        parse_line(&line, today(), &Utc, &known),
        LineResult::Malformed(Malformed::SeveralRaids)
    );
}

#[test]
fn newer_version_needs_update() {
    for machine in [
        "<BF2;A;SA0001;x>",
        "<BF2P;Q>",
        "<BF10;A;anything at all",
        "<BF99999999999999999999;A>",
    ] {
        assert_eq!(
            parse(&award_line(machine)),
            LineResult::NeedsUpdate,
            "{machine}"
        );
    }
}

#[test]
fn a_bad_line_does_not_affect_the_next() {
    let good = award_line("<BF1;A;SA0001;aw-9d2f6h1j;16805;ifron;HR;91;Zoé Brûlé>");
    let lines = [
        award_line("<BF1;A;SA0001;aw-9d2f6h1j;16805;ifron;HR;101;Zoé Brûlé>"),
        good.clone(),
        award_line("<BF2;A;SA0001>"),
        good.clone(),
        chat("<BF1;A;%;;;"),
        good,
    ];
    let results: Vec<_> = lines.iter().map(|l| parse(l)).collect();
    let award = LineResult::Operation(Operation::Award(spec_award(false)));
    assert_eq!(results[1], award);
    assert_eq!(results[3], award);
    assert_eq!(results[5], award);
    assert_eq!(results[0], LineResult::Malformed(Malformed::Roll));
    assert_eq!(results[2], LineResult::NeedsUpdate);
    assert!(matches!(results[4], LineResult::Malformed(_)));
}

#[test]
fn never_panics_on_odd_input() {
    let lines = [
        chat("<BF1"),
        chat("<BF1;"),
        chat("<BF1;A"),
        chat("<BF1;A;;;;;;;;>"),
        chat("<BF1;R;;;;;;;;;;>"),
        chat("é<BF1;V;é;é;é>é"),
        chat(&format!(
            "{LINK}|Hitem:é <BF1;V;SA0001;vd-2s9t4w7y;aw-9d2f6h1j>"
        )),
        "10/13 10:01:51.269  |Hchannel:RAID|h[Raid]|h : <BF1;A>".into(),
        "é é |Hchannel:RAID|h[é]|h é: <BF1é".into(),
    ];
    for line in lines {
        let _ = parse(&line);
    }
}

#[test]
fn empty_sender_is_malformed() {
    let line = format!(
        "10/13 10:01:51.269  |Hchannel:RAID|h[Raid]|h : {LINK} <BF1;V;SA0001;vd-2s9t4w7y;aw-9d2f6h1j>"
    );
    assert_eq!(malformed(&line), Malformed::Sender);
}

// ADDON-SPEC 9.3 escaping.

#[test]
fn decodes_escapes() {
    assert_eq!(decode("Zoé Brûlé").as_deref(), Some("Zoé Brûlé"));
    assert_eq!(
        decode("a%3Bb%25c%3C%3E%7C%0A").as_deref(),
        Some("a;b%c<>|\n")
    );
    assert_eq!(decode("%C3%A9").as_deref(), Some("é"));
    assert_eq!(decode("%c3%a9").as_deref(), Some("é"));
    assert_eq!(decode("100%"), None);
    assert_eq!(decode("%4"), None);
    assert_eq!(decode("%G1"), None);
    assert_eq!(decode("%+1"), None);
    assert_eq!(decode("%C3"), None);
    assert_eq!(decode("%FF%FE"), None);
}

#[test]
fn escaped_winner_in_a_line() {
    let line =
        award_line("<BF1;A;SA0001;aw-9d2f6h1j;16805;ifron;HR;91;Zo%C3%A9 %3BBr%C3%BBl%C3%A9>");
    let LineResult::Operation(Operation::Award(award)) = parse(&line) else {
        panic!("expected an award");
    };
    assert_eq!(award.char.as_deref(), Some("Zoé ;Brûlé"));
    let line = award_line("<BF1;A;SA0001;aw-9d2f6h1j;16805;ifron;HR;91;Zo%C3>");
    assert_eq!(malformed(&line), Malformed::Escape);
    let line = award_line("<BF1;A;SA0001;aw-9d2f6h1j;16805;ifron;HR;91;50% off>");
    assert_eq!(malformed(&line), Malformed::Escape);
}

// ADDON-SPEC 9.5 time.

fn at_of(line: &str, today: NaiveDate, tz: &impl TimeZone) -> i64 {
    match parse_line(line, today, tz, &raids()) {
        LineResult::Operation(Operation::Award(award)) => award.at,
        other => panic!("expected an award, got {other:?}"),
    }
}

fn dated(date: &str) -> String {
    format!(
        "{date} 23:59:58.001  |Hchannel:RAID|h[Raid]|h Rae Quill: {LINK} <BF1;A;SA0001;aw-9d2f6h1j;16805;ifron;HR;91;Zoé>"
    )
}

#[test]
fn year_is_the_one_the_date_was_most_recently_in() {
    let jan2 = NaiveDate::from_ymd_opt(2027, 1, 2).unwrap();
    // 2026-12-31 23:59:58 UTC: read on 2 January, New Year's Eve was last year.
    assert_eq!(at_of(&dated("12/31"), jan2, &Utc), 1_798_761_598);
    // 2027-01-02 23:59:58 UTC: today stays this year.
    assert_eq!(at_of(&dated("01/02"), jan2, &Utc), 1_798_934_398);
    // 2027-01-03 is tomorrow, so it means 2026-01-03 23:59:58 UTC.
    assert_eq!(at_of(&dated("01/03"), jan2, &Utc), 1_767_484_798);
}

#[test]
fn leap_day_takes_the_last_leap_year() {
    let day = NaiveDate::from_ymd_opt(2027, 3, 1).unwrap();
    let want = Utc
        .with_ymd_and_hms(2024, 2, 29, 23, 59, 58)
        .unwrap()
        .timestamp();
    assert_eq!(at_of(&dated("02/29"), day, &Utc), want);
}

#[test]
fn log_time_is_read_in_the_given_zone() {
    let paris = FixedOffset::east_opt(2 * 3600).unwrap();
    let la = FixedOffset::west_opt(7 * 3600).unwrap();
    assert_eq!(
        at_of(&dated("10/13"), today(), &paris),
        at_of(&dated("10/13"), today(), &Utc) - 2 * 3600
    );
    assert_eq!(
        at_of(&dated("10/13"), today(), &la),
        at_of(&dated("10/13"), today(), &Utc) + 7 * 3600
    );
}

#[test]
fn parse_line_now_uses_this_computers_zone() {
    let now = Local::now();
    let line = format!(
        "{} 12:00:00.000  |Hchannel:RAID|h[Raid]|h Rae Quill: {LINK} <BF1;A;SA0001;aw-9d2f6h1j;16805;ifron;HR;91;Zoé>",
        now.format("%m/%d")
    );
    let want = Local
        .from_local_datetime(&now.date_naive().and_hms_opt(12, 0, 0).unwrap())
        .earliest()
        .unwrap()
        .timestamp();
    let LineResult::Operation(Operation::Award(award)) = parse_line_now(&line, &raids()) else {
        panic!("expected an award");
    };
    assert_eq!(award.at, want);
}
