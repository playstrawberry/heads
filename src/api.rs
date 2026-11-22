//! The API for other plugins: ask `strawberry-heads` for a player's head over the plugin
//! messages (`ipc.send-ipc-message`).
//!
//! Request (UTF-8): `head <name or uuid>`. Answer: `ok`, then tab-separated the UUID, the name,
//! the base64 `textures` value and its signature (both empty for a player without a skin), or
//! an error text for a player the cache has never seen. The caller makes the head itself with
//! `TextComponent::player_head` (name, id, textures, signature), so the head is its own text
//! component with its own style.

use crate::cache::{Head, HeadCache};

/// What a request asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Request<'a> {
    Head(&'a str),
}

/// Reads a request.
pub fn parse(message: &[u8]) -> Result<Request<'_>, String> {
    let text = std::str::from_utf8(message).map_err(|_| "a request is UTF-8 text".to_owned())?;
    match text.split_once(' ') {
        Some(("head", who)) if !who.trim().is_empty() => Ok(Request::Head(who.trim())),
        _ => Err("unknown request; write `head <name or uuid>`".to_owned()),
    }
}

/// The answer for a head.
#[must_use]
pub fn answer(head: &Head) -> Vec<u8> {
    format!(
        "ok\t{}\t{}\t{}\t{}",
        head.id,
        head.name,
        head.textures.as_deref().unwrap_or(""),
        head.signature.as_deref().unwrap_or("")
    )
    .into_bytes()
}

/// Answers a request from the cache.
pub fn handle(cache: &HeadCache, message: &[u8]) -> Result<Vec<u8>, String> {
    match parse(message)? {
        Request::Head(who) => cache
            .find(who)
            .map(answer)
            .ok_or_else(|| format!("no head known for {who}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache() -> HeadCache {
        let mut cache = HeadCache::default();
        cache.see(Head {
            id: "8667ba71-b85a-4004-af54-457a9734eed7".into(),
            name: "Steve".into(),
            seen: 1,
            textures: Some("dGV4".into()),
            signature: Some("c2ln".into()),
        });
        cache.see(Head {
            id: "00000000-0000-0000-0000-000000000001".into(),
            name: "Cracked".into(),
            seen: 1,
            textures: None,
            signature: None,
        });
        cache
    }

    #[test]
    fn a_known_player_is_answered_with_the_profile_fields() {
        let answer = handle(&cache(), b"head steve").unwrap();
        assert_eq!(
            String::from_utf8(answer).unwrap(),
            "ok\t8667ba71-b85a-4004-af54-457a9734eed7\tSteve\tdGV4\tc2ln"
        );
        let answer = handle(&cache(), b"head 00000000000000000000000000000001").unwrap();
        assert_eq!(
            String::from_utf8(answer).unwrap(),
            "ok\t00000000-0000-0000-0000-000000000001\tCracked\t\t"
        );
    }

    #[test]
    fn unknown_players_and_bad_requests_are_errors() {
        assert!(handle(&cache(), b"head Alex").unwrap_err().contains("Alex"));
        assert!(handle(&cache(), b"head ").is_err());
        assert!(handle(&cache(), b"tail Steve").is_err());
        assert!(handle(&cache(), &[0xff, 0xfe]).is_err());
    }
}
