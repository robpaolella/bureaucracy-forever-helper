use super::*;
use serde_json::json;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A fresh, empty folder for one test.
fn scratch_dir() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "bf-inbox-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn files_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// ADDON-SPEC 3.3's worked example, as the helper API sends it (7.1).
fn worked_example() -> Value {
    json!({
        "formatVersion": 1, "raid": "cm9xr2k4a0001", "code": "SA0001", "env": "staging",
        "name": "Molten Core - sample", "startsAt": 1792000000, "durationMin": 240,
        "instance": 409, "generated": 1791990000,
        "addon": { "version": "0.1.0",
            "url": "https://github.com/robpaolella/bureaucracy-forever-addon/releases/latest" },
        "bosses": [
            { "id": "lb_lucifron", "name": "Lucifron", "trash": false, "items": [16800, 16805] },
            { "id": "lb_magmadar", "name": "Magmadar", "trash": false, "items": [16796, 16805] },
            { "id": "lb_trash", "name": "Trash", "trash": true, "items": [17011, 18861] }
        ],
        "reserves": {
            "16805": [
                { "user": "u_ana", "char": "Ana Vale", "kind": "HR", "trial": false },
                { "user": "u_bo", "char": "Bo Marsh", "kind": "SR", "trial": false },
                { "user": "u_cy", "char": "Cy Orchard", "kind": "SR", "trial": true }
            ],
            "16796": [
                { "user": "u_bo", "char": "Bo Marsh", "kind": "HR", "trial": false,
                  "blocked": "win-limit" }
            ]
        },
        "awards": [
            { "key": "site-0001abcd", "boss": "lb_trash", "item": 18861, "char": "Dee Holt",
              "user": "u_dee", "method": "OPEN_ROLL", "roll": 64, "voided": false }
        ]
    })
}

fn sample_catchup() -> Value {
    json!({
        "formatVersion": 1, "raid": "cm9xr2k4a0001", "env": "staging", "generated": 1791990050,
        "awards": [
            { "key": "site-0001abcd", "boss": "lb_trash", "item": 18861, "char": "Dee Holt",
              "user": "u_dee", "method": "OPEN_ROLL", "roll": 64, "voided": false,
              "state": "manual", "at": 1791990040 }
        ],
        "voids": [ { "key": "k-0002", "reason": "wrong person", "at": 1791990045 } ]
    })
}

fn raid(snapshot: Value) -> RaidData {
    RaidData {
        snapshot,
        catchup: None,
    }
}

fn snapshot_with_name(name: &str) -> Value {
    let mut s = worked_example();
    s["name"] = json!(name);
    s
}

const TRICKY: &[&str] = &[
    "say \"hi\"",
    "back\\slash",
    "line\nbreak",
    "]]",
    "--",
    "-- comment",
    "\\z",
    "a]]--[[b",
    "\"); os.exit() --",
    "Zoë Ångström",
    "tab\there",
    "del\u{7f}",
    "1\u{0}2",
    "",
];

fn render_one(snapshot: Value) -> Result<Vec<u8>, InboxError> {
    render_inbox(&[raid(snapshot)], 1791990100)
}

fn bad_data(result: Result<Vec<u8>, InboxError>) -> String {
    match result {
        Err(InboxError::BadData { raid, problem }) => format!("{raid}: {problem}"),
        other => panic!("expected bad data, got {other:?}"),
    }
}

#[test]
fn strings_are_written_inertly_and_read_back() {
    let mut snapshot = worked_example();
    snapshot["bosses"][0]["items"] = json!([]);
    for (i, s) in TRICKY.iter().enumerate() {
        snapshot["bosses"][i % 3]["name"] = json!(s);
        snapshot["awards"][0]["char"] = json!(s);
        let out = render_one(snapshot.clone()).unwrap();
        assert!(
            out[..out.len() - 1]
                .iter()
                .all(|b| (0x20..=0x7e).contains(b)),
            "raw byte in output for {s:?}"
        );
        let back = parse_inbox(&out).unwrap();
        let Lua::Table(top) = &back else { panic!() };
        let Lua::Table(raids) = &top[2].1 else {
            panic!()
        };
        let Lua::Table(entry) = &raids[0].1 else {
            panic!()
        };
        let Lua::Table(snap) = &entry[0].1 else {
            panic!()
        };
        let awards = snap
            .iter()
            .find(|f| f.0 == Some(Key::Name("awards")))
            .unwrap();
        let Lua::Table(awards) = &awards.1 else {
            panic!()
        };
        let Lua::Table(award) = &awards[0].1 else {
            panic!()
        };
        let char_field = award
            .iter()
            .find(|f| f.0 == Some(Key::Name("char")))
            .unwrap();
        assert_eq!(char_field.1, Lua::Str(s.to_string()));
    }
}

#[test]
fn escapes_follow_the_spec_exactly() {
    let mut out = Vec::new();
    write_value(&Lua::Str("a\"b\\c\nd]]--\\zé\u{7f}1".into()), &mut out);
    assert_eq!(
        String::from_utf8(out).unwrap(),
        r#""a\"b\\c\010d]]--\\z\195\169\1271""#
    );
}

#[test]
fn worked_example_is_written_as_expected() {
    let out = render_one(worked_example()).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.starts_with(
        "BureaucracyLootInbox = {formatVersion=1,written=1791990100,raids={{snapshot={addon={"
    ));
    assert!(text.contains("reserves={[16796]={{blocked=\"win-limit\",char=\"Bo Marsh\""));
    assert!(text.contains("[16805]={{char=\"Ana Vale\",kind=\"HR\",trial=false,user=\"u_ana\"}"));
    assert!(text.contains("items={16800,16805}"));
    assert!(text.ends_with("}}}}\n"));
}

#[test]
fn unknown_keys_are_refused() {
    let mut s = worked_example();
    s["note"] = json!("x");
    assert!(bad_data(render_one(s)).contains("field named \"note\""));

    for key in [
        "016805",
        "+5",
        "-0",
        "1.5",
        "a b",
        "x=1}",
        "99999999999999999",
    ] {
        let mut s = worked_example();
        s["reserves"][key] = json!([]);
        let err = bad_data(render_one(s));
        assert!(
            err.contains("field named") || err.contains("too large"),
            "{key}: {err}"
        );
    }
}

#[test]
fn negative_and_boundary_integers_are_kept() {
    let mut s = worked_example();
    s["reserves"]["-3"] = json!([]);
    s["startsAt"] = json!(MAX_INTEGER);
    s["generated"] = json!(-MAX_INTEGER);
    let out = render_one(s).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("[-3]={}"));
    assert!(text.contains("startsAt=9007199254740991"));
    assert!(text.contains("generated=-9007199254740991"));
}

#[test]
fn floats_nulls_and_big_numbers_are_refused() {
    let mut s = worked_example();
    s["durationMin"] = json!(240.5);
    assert!(bad_data(render_one(s)).contains("240.5"));

    let mut s = worked_example();
    s["durationMin"] = serde_json::from_str("240.0").unwrap();
    assert!(bad_data(render_one(s)).contains("not a whole number"));

    let mut s = worked_example();
    s["instance"] = Value::Null;
    assert!(bad_data(render_one(s)).contains("null"));

    for n in [MAX_INTEGER + 1, -MAX_INTEGER - 1] {
        let mut s = worked_example();
        s["startsAt"] = json!(n);
        assert!(bad_data(render_one(s)).contains("too large"));
    }
    let mut s = worked_example();
    s["startsAt"] = json!(u64::MAX);
    assert!(bad_data(render_one(s)).contains("not a whole number"));

    assert!(matches!(
        render_inbox(&[], MAX_INTEGER + 1),
        Err(InboxError::BadData { .. })
    ));
}

#[test]
fn depth_over_eight_is_refused() {
    // Snapshot is depth 4; reserves 5, holders list 6, holder 7, so one more table is 8.
    let mut s = worked_example();
    s["reserves"]["1"] = json!([{ "items": [] }]);
    assert!(render_one(s).is_ok());

    let mut s = worked_example();
    s["reserves"]["1"] = json!([{ "items": [[]] }]);
    assert!(bad_data(render_one(s)).contains("more than 8 levels"));
}

#[test]
fn string_over_200_bytes_is_refused_naming_the_raid() {
    let mut s = worked_example();
    s["awards"][0]["char"] = json!("é".repeat(100));
    assert!(render_one(s.clone()).is_ok());

    s["awards"][0]["char"] = json!(format!("{}x", "é".repeat(100)));
    let dir = scratch_dir();
    let target = dir.join("Inbox.lua");
    fs::write(&target, "old").unwrap();
    let err = write_inbox(&target, &[raid(s)], 1).unwrap_err().to_string();
    assert!(err.contains("Molten Core - sample (SA0001)"), "{err}");
    assert!(err.contains("201 bytes"), "{err}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "old");
    assert_eq!(files_in(&dir), ["Inbox.lua"]);
}

/// A snapshot whose entry is about `bytes` long.
fn big_snapshot(name: &str, bytes: usize) -> Value {
    let mut s = snapshot_with_name(name);
    let holder = json!({ "user": "u_x", "char": "x".repeat(180), "kind": "SR", "trial": false });
    let per = serde_json::to_string(&holder).unwrap().len();
    s["reserves"]["1"] = json!(vec![holder; bytes / per]);
    s
}

#[test]
fn raid_entry_over_200000_bytes_is_refused_and_old_file_kept() {
    let dir = scratch_dir();
    let target = dir.join("Inbox.lua");
    fs::write(&target, "old").unwrap();

    let raids = [
        raid(snapshot_with_name("Small")),
        raid(big_snapshot("Huge Raid", 210_000)),
    ];
    let err = write_inbox(&target, &raids, 1).unwrap_err();
    let InboxError::RaidTooBig { raid, bytes } = &err else {
        panic!("{err:?}")
    };
    assert_eq!(raid, "Huge Raid (SA0001)");
    assert!(*bytes > MAX_RAID_BYTES);
    let msg = err.to_string();
    assert!(
        msg.contains("Huge Raid (SA0001)") && msg.contains("200,000"),
        "{msg}"
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), "old");
    assert_eq!(files_in(&dir), ["Inbox.lua"]);
}

#[test]
fn file_over_700000_bytes_is_refused_and_old_file_kept() {
    let dir = scratch_dir();
    let target = dir.join("Inbox.lua");
    fs::write(&target, "old").unwrap();

    let raids: Vec<RaidData> = (0..4)
        .map(|i| raid(big_snapshot(&format!("Raid {i}"), 190_000)))
        .collect();
    let err = write_inbox(&target, &raids, 1).unwrap_err();
    let InboxError::FileTooBig { bytes } = err else {
        panic!("{err:?}")
    };
    assert!(bytes > MAX_FILE_BYTES);
    assert!(err_text(bytes).contains("700,000"));
    assert_eq!(fs::read_to_string(&target).unwrap(), "old");
    assert_eq!(files_in(&dir), ["Inbox.lua"]);

    // Three at the same size fit.
    assert!(render_inbox(&raids[..3], 1).is_ok());
}

fn err_text(bytes: usize) -> String {
    InboxError::FileTooBig { bytes }.to_string()
}

#[test]
fn no_raids_writes_an_empty_list() {
    assert_eq!(
        render_inbox(&[], 5).unwrap(),
        b"BureaucracyLootInbox = {formatVersion=1,written=5,raids={}}\n"
    );
}

#[test]
fn raids_keep_the_given_order_with_catchups() {
    let raids = [
        raid(snapshot_with_name("Zed")),
        RaidData {
            snapshot: snapshot_with_name("Alpha"),
            catchup: Some(sample_catchup()),
        },
        raid(snapshot_with_name("Mid")),
    ];
    let text = String::from_utf8(render_inbox(&raids, 1).unwrap()).unwrap();
    let pos = |s: &str| text.find(s).unwrap();
    assert!(pos("\"Zed\"") < pos("\"Alpha\"") && pos("\"Alpha\"") < pos("\"Mid\""));
    assert_eq!(text.matches("catchup=").count(), 1);
    assert!(pos("\"Alpha\"") < pos("catchup=") && pos("catchup=") < pos("\"Mid\""));
}

#[test]
fn same_input_gives_same_bytes() {
    let raids = || {
        [RaidData {
            snapshot: worked_example(),
            catchup: Some(sample_catchup()),
        }]
    };
    assert_eq!(
        render_inbox(&raids(), 7).unwrap(),
        render_inbox(&raids(), 7).unwrap()
    );

    // Key order in the site's JSON doesn't change the bytes.
    let reordered: Value =
        serde_json::from_str(r#"{"trial": false, "kind": "SR", "char": "B", "user": "u"}"#)
            .unwrap();
    let mut a = worked_example();
    let mut b = worked_example();
    a["reserves"]["1"] = json!([{ "user": "u", "char": "B", "kind": "SR", "trial": false }]);
    b["reserves"]["1"] = json!([reordered]);
    assert_eq!(render_one(a).unwrap(), render_one(b).unwrap());

    let dir = scratch_dir();
    let target = dir.join("Inbox.lua");
    write_inbox(&target, &raids(), 7).unwrap();
    let first = fs::read(&target).unwrap();
    write_inbox(&target, &raids(), 7).unwrap();
    assert_eq!(fs::read(&target).unwrap(), first);
}

#[test]
fn writing_replaces_the_old_file_and_leaves_no_temp() {
    let dir = scratch_dir();
    let target = dir.join("Inbox.lua");
    fs::write(&target, "old").unwrap();
    write_inbox(&target, &[raid(worked_example())], 9).unwrap();
    assert_eq!(fs::read(&target).unwrap(), render_one_at(9));
    assert_eq!(files_in(&dir), ["Inbox.lua"]);

    // Also when there was no old file.
    let fresh = dir.join("New.lua");
    write_inbox(&fresh, &[], 9).unwrap();
    assert_eq!(files_in(&dir), ["Inbox.lua", "New.lua"]);
}

fn render_one_at(written: i64) -> Vec<u8> {
    render_inbox(&[raid(worked_example())], written).unwrap()
}

#[test]
fn a_failed_write_keeps_the_old_file_and_removes_the_temp() {
    let dir = scratch_dir();
    let target = dir.join("Inbox.lua");
    fs::write(&target, "old").unwrap();
    let result = replace_file(
        &target,
        |file| {
            file.write_all(b"BureaucracyLootInbox = {")?;
            Err(io::Error::other("disk full"))
        },
        |_| true,
        |from, to| fs::rename(from, to),
    );
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&target).unwrap(), "old");
    assert_eq!(files_in(&dir), ["Inbox.lua"]);
}

#[test]
fn a_failed_swap_keeps_the_old_file_and_removes_the_temp() {
    let dir = scratch_dir();
    let target = dir.join("Inbox.lua");
    fs::write(&target, "old").unwrap();
    let result = replace_file(
        &target,
        |file| file.write_all(b"new"),
        |_| true,
        |_, _| Err(io::Error::other("locked")),
    );
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&target).unwrap(), "old");
    assert_eq!(files_in(&dir), ["Inbox.lua"]);

    // A real rename failure: the target is a folder with something in it.
    let blocked = dir.join("Blocked.lua");
    fs::create_dir(&blocked).unwrap();
    fs::write(blocked.join("keep"), "x").unwrap();
    let err = write_inbox(&blocked, &[], 1).unwrap_err();
    assert!(matches!(err, InboxError::Io(_)), "{err:?}");
    assert!(err
        .to_string()
        .starts_with("The reserves couldn't be saved"));
    assert_eq!(files_in(&blocked), ["keep"]);
    assert_eq!(files_in(&dir), ["Blocked.lua", "Inbox.lua"]);
}

#[test]
fn a_temp_file_that_reads_back_differently_is_not_swapped_in() {
    let dir = scratch_dir();
    let target = dir.join("Inbox.lua");
    fs::write(&target, "old").unwrap();
    let (expected, _) = render(&[], 1).unwrap();
    let other = render_inbox(&[], 2).unwrap();
    let result = replace_file(
        &target,
        |file| file.write_all(&other),
        |back| parse_inbox(back).as_ref() == Ok(&expected),
        |from, to| fs::rename(from, to),
    );
    assert!(
        matches!(result, Err(InboxError::ReadBackMismatch)),
        "{result:?}"
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), "old");
    assert_eq!(files_in(&dir), ["Inbox.lua"]);
}

#[test]
fn existing_files_and_links_are_never_used_as_the_temp_file() {
    let dir = scratch_dir();
    let target = dir.join("Inbox.lua");
    fs::write(&target, "old").unwrap();
    let name = |n: u64| dir.join(format!("Inbox.lua.{}-{n}.tmp", std::process::id()));
    fs::write(name(0), "stray").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, name(1)).unwrap();
    #[cfg(not(unix))]
    fs::write(name(1), "stray").unwrap();

    let (temp, mut file) = create_temp(&target, 0).unwrap();
    assert_eq!(temp, name(2));
    file.write_all(b"new").unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "old");
    assert_eq!(fs::read_to_string(name(0)).unwrap(), "stray");
}

#[test]
fn overlapping_writes_each_swap_in_a_whole_file() {
    let dir = scratch_dir();
    let target = dir.join("Inbox.lua");
    let renders: Vec<Vec<u8>> = (0..4)
        .map(|i| render_inbox(&[raid(big_snapshot("Raid", 20_000 * (i + 1)))], 1).unwrap())
        .collect();
    std::thread::scope(|scope| {
        for i in 0..4 {
            let target = &target;
            scope.spawn(move || {
                for _ in 0..25 {
                    write_inbox(target, &[raid(big_snapshot("Raid", 20_000 * (i + 1)))], 1)
                        .unwrap();
                }
            });
        }
        // While they run, every read sees one whole file.
        for _ in 0..200 {
            if let Ok(now) = fs::read(&target) {
                assert!(renders.contains(&now), "read a mixed file");
            }
        }
    });
    assert!(renders.contains(&fs::read(&target).unwrap()));
    assert_eq!(files_in(&dir), ["Inbox.lua"]);
}

#[test]
fn a_missing_folder_is_an_error_not_a_panic() {
    let dir = scratch_dir();
    let err = write_inbox(&dir.join("gone").join("Inbox.lua"), &[], 1).unwrap_err();
    assert!(matches!(err, InboxError::Io(_)));
}

#[test]
fn the_parser_accepts_only_the_data_grammar() {
    let ok = |s: &str| parse_inbox(s.as_bytes());
    assert!(
        ok("BureaucracyLootInbox = { raids = { { name = \"a\" , [3] = -1 , true, }, } }").is_ok()
    );
    for bad in [
        "BureaucracyLootInbox = {} x = 1",
        "BureaucracyLootInbox = {name = f()}",
        "BureaucracyLootInbox = {name = \"a\" .. \"b\"}",
        "BureaucracyLootInbox = {name = \"a\"} -- note",
        "BureaucracyLootInbox = {name = [[a]]}",
        "BureaucracyLootInbox = {name = 'a'}",
        "BureaucracyLootInbox = {name = \"a\\n\"}",
        "BureaucracyLootInbox = {name = \"a\\z\"}",
        "BureaucracyLootInbox = {name = \"a\\256\"}",
        "BureaucracyLootInbox = {name = \"a\nb\"}",
        "BureaucracyLootInbox = {name = 1.5}",
        "BureaucracyLootInbox = {name = nil}",
        "BureaucracyLootInbox = {name = os}",
        "BureaucracyLootInbox = {[\"name\"] = 1}",
        "BureaucracyLootInbox = {note = 1}",
        "BureaucracyLootInbox = {raids = 9007199254740992}",
        "BureaucracyLootInbox = {{{{{{{{{}}}}}}}}}",
        "Other = {}",
        "BureaucracyLootInbox = \"x\"",
        "BureaucracyLootInbox = {name = \"a\"",
    ] {
        assert!(ok(bad).is_err(), "accepted: {bad}");
    }
    assert!(ok("BureaucracyLootInbox = {{{{{{{{}}}}}}}}").is_ok());
}

/// Loads `inbox` in a real Lua 5.1 with nothing else in reach, checks it set only the
/// inbox global, and returns its contents in a canonical dump.
fn lua_dump(inbox: &Path) -> Option<String> {
    let script = r#"
        local f = assert(loadfile(arg[1]))
        local env = {}
        setfenv(f, env)
        f()
        for k in pairs(env) do
          assert(k == "BureaucracyLootInbox", "unexpected global " .. tostring(k))
        end
        local function hex(s)
          return (s:gsub(".", function(c) return string.format("%02x", c:byte()) end))
        end
        local function dump(v)
          local t = type(v)
          if t == "string" then return "s" .. hex(v)
          elseif t == "number" then
            assert(v == math.floor(v), "not an integer")
            return "n" .. string.format("%d", v)
          elseif t == "boolean" then return "b" .. tostring(v)
          elseif t == "table" then
            local parts = {}
            for k, x in pairs(v) do parts[#parts + 1] = dump(k) .. "=" .. dump(x) end
            table.sort(parts)
            return "{" .. table.concat(parts, ";") .. "}"
          end
          error("unexpected type " .. t)
        end
        io.write(dump(env.BureaucracyLootInbox))
    "#;
    let output = match Command::new("lua5.1")
        .arg("-e")
        .arg(format!("arg = {{ [1] = {:?} }}", inbox.to_str().unwrap()))
        .arg("-e")
        .arg(script)
        .output()
    {
        Ok(output) => output,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
        Err(e) => panic!("couldn't run lua5.1: {e}"),
    };
    assert!(
        output.status.success(),
        "lua5.1 failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Some(String::from_utf8(output.stdout).unwrap())
}

/// The same canonical dump, made from the data tree. Positional entries are keys 1, 2, …
fn rust_dump(value: &Lua) -> String {
    let hex = |s: &[u8]| s.iter().map(|b| format!("{b:02x}")).collect::<String>();
    match value {
        Lua::Str(s) => format!("s{}", hex(s.as_bytes())),
        Lua::Int(i) => format!("n{i}"),
        Lua::Bool(b) => format!("b{b}"),
        Lua::Table(fields) => {
            let mut position = 0;
            let mut parts: Vec<String> = fields
                .iter()
                .map(|(k, v)| {
                    let key = match k {
                        Some(Key::Name(n)) => format!("s{}", hex(n.as_bytes())),
                        Some(Key::Int(i)) => format!("n{i}"),
                        None => {
                            position += 1;
                            format!("n{position}")
                        }
                    };
                    format!("{key}={}", rust_dump(v))
                })
                .collect();
            parts.sort();
            format!("{{{}}}", parts.join(";"))
        }
    }
}

#[test]
fn output_loads_in_lua_5_1_as_the_same_data() {
    let mut tricky = worked_example();
    for (i, s) in TRICKY.iter().enumerate() {
        tricky["bosses"][i % 3]["items"] = json!([]);
        tricky["reserves"][(1000 + i).to_string()] =
            json!([{ "user": "u", "char": s, "kind": "SR", "trial": false }]);
    }
    let cases = [
        vec![RaidData {
            snapshot: worked_example(),
            catchup: Some(sample_catchup()),
        }],
        vec![raid(tricky)],
        vec![],
    ];
    let dir = scratch_dir();
    for (i, raids) in cases.iter().enumerate() {
        let target = dir.join(format!("Inbox{i}.lua"));
        write_inbox(&target, raids, 1791990100).unwrap();
        let Some(from_lua) = lua_dump(&target) else {
            // Written straight to stderr so it shows even though the test passes.
            let _ = writeln!(
                io::stderr(),
                "SKIPPED output_loads_in_lua_5_1_as_the_same_data: lua5.1 is not installed"
            );
            return;
        };
        let expected = parse_inbox(&fs::read(&target).unwrap()).unwrap();
        assert_eq!(from_lua, rust_dump(&expected), "case {i}");
    }
    // The dump really compares content: the worked example's holder is in it.
    let ana = rust_dump(&Lua::Str("Ana Vale".into()));
    assert!(lua_dump(&dir.join("Inbox0.lua")).unwrap().contains(&ana));
}

#[test]
fn compare_on_disk_ignores_the_written_time_only() {
    let dir = scratch_dir();
    let target = dir.join("Inbox.lua");
    let raids = [RaidData {
        snapshot: worked_example(),
        catchup: Some(sample_catchup()),
    }];
    assert_eq!(compare_on_disk(&target, &raids), OnDisk::Missing);

    write_inbox(&target, &raids, 100).unwrap();
    assert_eq!(compare_on_disk(&target, &raids), OnDisk::Same);

    let other = [RaidData {
        snapshot: snapshot_with_name("Blackwing Lair"),
        catchup: Some(sample_catchup()),
    }];
    assert_eq!(compare_on_disk(&target, &other), OnDisk::Different);
    assert_eq!(compare_on_disk(&target, &[]), OnDisk::Different);

    fs::write(&target, "-- not an inbox").unwrap();
    assert_eq!(compare_on_disk(&target, &raids), OnDisk::Different);
}

#[test]
fn a_write_waits_while_the_add_on_folder_is_being_swapped() {
    let dir = scratch_dir();
    let target = dir.join("Inbox.lua");
    let guard = ADDON_FOLDER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let writer = {
        let target = target.clone();
        std::thread::spawn(move || write_inbox(&target, &[], 1).unwrap())
    };
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(!target.exists(), "it wrote while the folder was locked");
    drop(guard);
    writer.join().unwrap();
    assert!(target.exists());
}
