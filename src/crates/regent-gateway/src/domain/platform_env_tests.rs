//! One table-driven contract test for every messaging platform: the names the
//! writers accept must be exactly the names the runtime reads.
//!
//! The test derives both sides instead of restating them, so it fails on real
//! drift rather than on a stale copy of a list:
//!
//! - the **writer** side comes from [`regent_tools::MANAGED`] (the same catalog
//!   `regent keys`, the agent key tool, and the Desktop page enforce);
//! - the **reader** side is extracted from the deacon's registry sources, which
//!   contain nothing but platform-credential reads, so *everything* they read
//!   has to be in the contract.
//!
//! Two of the seventeen platforms are composed outside those registry files
//! (Telegram in the gateway binary, Discord's interactions route in the deacon's
//! HTTP server); for those the scan can only confirm the declared name is read,
//! not discover an undeclared one. That is called out per source below.

use super::{NOT_SETTABLE_BY_DESIGN, PLATFORM_CREDENTIALS, UNREAD_BY_RUNTIME};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Sources that read *only* platform credentials — every name they read must
/// appear in the contract.
const EXHAUSTIVE_READER_SOURCES: &[&str] = &[
    "../regent-deacon/src/infra/webhook/registry.rs",
    "../regent-deacon/src/infra/webhook/registry_ext.rs",
];

/// Sources that read a mix of credentials and unrelated configuration — used to
/// confirm a declared name really is read, never to discover new ones.
const PARTIAL_READER_SOURCES: &[&str] = &[
    "../regent-deacon/src/application/http_serve.rs",
    "src/bin/gateway/main.rs",
    "src/domain/auth.rs",
];

fn source(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("contract test cannot read {}: {error}", path.display()))
}

/// Every `SCREAMING_SNAKE` literal passed to a `var(...)` / `env::var(...)`
/// call in `text`. Deliberately textual: the readers live in another crate and
/// the whole point is to catch a renamed string, which no type system sees.
fn env_names_read(text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for tail in text.split("var(\"").skip(1) {
        let Some((name, _)) = tail.split_once('"') else {
            continue;
        };
        if !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        {
            names.insert(name.to_owned());
        }
    }
    names
}

#[test]
fn messaging_credential_names_match_between_writers_and_runtime() {
    let managed: BTreeSet<&str> = regent_tools::MANAGED
        .iter()
        .map(|(name, _)| *name)
        .collect();
    let declared: BTreeSet<&str> = PLATFORM_CREDENTIALS
        .iter()
        .flat_map(|(_, names)| names.iter().copied())
        .collect();
    let unread: BTreeSet<&str> = UNREAD_BY_RUNTIME.iter().map(|(name, _)| *name).collect();
    let mut faults: Vec<String> = Vec::new();

    // 1. Writer side, per platform: the catalog must accept every name the
    //    runtime reads, and file it under the messaging group so the CLI and
    //    the Desktop page put it where a user looks for it.
    for (platform, names) in PLATFORM_CREDENTIALS {
        for name in *names {
            if !managed.contains(name) {
                faults.push(format!(
                    "{platform}: runtime reads {name} but the managed catalog will not set it \
                     (add it to regent-tools catalog.rs AND regent-cli keyCatalog.ts)"
                ));
            } else if regent_tools::key_group(name) != "messaging" {
                faults.push(format!(
                    "{platform}: {name} is grouped as \"{}\", not \"messaging\"",
                    regent_tools::key_group(name)
                ));
            }
        }
    }

    // 2. The reverse: a messaging row no platform claims is a credential the
    //    user can save and nothing will ever consume.
    for name in managed
        .iter()
        .filter(|n| regent_tools::key_group(n) == "messaging")
    {
        if !declared.contains(name) {
            faults.push(format!(
                "catalog offers {name} but no platform in PLATFORM_CREDENTIALS claims it"
            ));
        }
    }

    // 3. Reader side. The registry sources are pure credential readers, so the
    //    diff runs both ways against them.
    let mut read: BTreeSet<String> = BTreeSet::new();
    for relative in EXHAUSTIVE_READER_SOURCES {
        let found = env_names_read(&source(relative));
        for name in &found {
            if !declared.contains(name.as_str()) {
                faults.push(format!(
                    "{relative} reads {name}, which is not in PLATFORM_CREDENTIALS"
                ));
            }
        }
        read.extend(found);
    }
    for relative in PARTIAL_READER_SOURCES {
        read.extend(
            env_names_read(&source(relative))
                .into_iter()
                .filter(|name| declared.contains(name.as_str())),
        );
    }
    for (platform, names) in PLATFORM_CREDENTIALS {
        for name in *names {
            if !read.contains(*name) && !unread.contains(name) {
                faults.push(format!(
                    "{platform}: {name} is declared and settable, but no runtime source reads it"
                ));
            }
        }
    }

    // 4. Auth posture stays out of the catalog: a credential-shaped name is not
    //    authority to widen who may talk to the agent.
    for name in NOT_SETTABLE_BY_DESIGN {
        assert!(
            !managed.contains(name),
            "{name} is auth posture and must not be settable through the key tool"
        );
    }

    assert!(
        faults.is_empty(),
        "credential-name contract broken:\n  {}",
        faults.join("\n  ")
    );
}

/// The exemption list is a promise to wire something up, not a parking lot: a
/// name listed there must still be settable and must still be unread.
#[test]
fn unread_names_are_documented_and_really_unread() {
    let read: BTreeSet<String> = EXHAUSTIVE_READER_SOURCES
        .iter()
        .chain(PARTIAL_READER_SOURCES)
        .flat_map(|relative| env_names_read(&source(relative)))
        .collect();
    for (name, reason) in UNREAD_BY_RUNTIME {
        assert!(!reason.is_empty(), "{name} needs a reason");
        assert!(
            !read.contains(*name),
            "{name} is now read at runtime — drop it from UNREAD_BY_RUNTIME"
        );
    }
}
