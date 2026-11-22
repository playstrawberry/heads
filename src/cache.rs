//! The shared head cache: the last skin seen for each player, by UUID and by name.
//!
//! An entry is refreshed whenever the player is seen (join, chat, `/heads refresh`), from the
//! skin the server holds for the player at that moment: the Mojang skin of an online-mode
//! server, the engine's offline skin lookup or a SkinsRestorer skin on an offline server. It is
//! kept in `data/heads.tsv` (one player a line: uuid, name, seen, textures, signature, separated
//! by tabs; base64 and names hold no tab), so other plugins can ask for the head of a player who
//! is not online.

use std::collections::HashMap;

/// Most players kept; the ones seen longest ago go first.
pub const MAX_ENTRIES: usize = 10_000;

/// One player's head.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Head {
    /// The UUID as text with dashes, lower case.
    pub id: String,
    pub name: String,
    /// When the player was last seen, in seconds (the caller's clock).
    pub seen: u64,
    /// The base64 `textures` value, if the player has a skin.
    pub textures: Option<String>,
    pub signature: Option<String>,
}

/// The cache.
#[derive(Debug, Default)]
pub struct HeadCache {
    by_id: HashMap<String, Head>,
}

impl HeadCache {
    /// Puts a seen player in; returns whether anything a file would keep changed (a new player,
    /// a new name or skin; only a newer `seen` is not worth a write).
    pub fn see(&mut self, head: Head) -> bool {
        let changed = self.by_id.get(&head.id).is_none_or(|old| {
            old.name != head.name
                || old.textures != head.textures
                || old.signature != head.signature
        });
        self.by_id.insert(head.id.clone(), head);
        if self.by_id.len() > MAX_ENTRIES {
            self.trim();
        }
        changed
    }

    fn trim(&mut self) {
        let mut seen: Vec<(u64, String)> = self
            .by_id
            .values()
            .map(|h| (h.seen, h.id.clone()))
            .collect();
        seen.sort_unstable();
        let extra = self.by_id.len() - MAX_ENTRIES;
        for (_, id) in seen.into_iter().take(extra) {
            self.by_id.remove(&id);
        }
    }

    /// The head of the player with this UUID (any case, with or without dashes) or this name (any
    /// case; the one seen last when two players had the name).
    #[must_use]
    pub fn find(&self, who: &str) -> Option<&Head> {
        let id = normal_id(who);
        if let Some(head) = id.as_deref().and_then(|id| self.by_id.get(id)) {
            return Some(head);
        }
        self.by_id
            .values()
            .filter(|h| h.name.eq_ignore_ascii_case(who))
            .max_by_key(|h| h.seen)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// The file text.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut heads: Vec<&Head> = self.by_id.values().collect();
        heads.sort_by(|a, b| a.id.cmp(&b.id));
        let mut out = String::new();
        for h in heads {
            out.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\n",
                h.id,
                h.name,
                h.seen,
                h.textures.as_deref().unwrap_or(""),
                h.signature.as_deref().unwrap_or("")
            ));
        }
        out
    }

    /// Reads the file text; lines that are not five well-formed fields are skipped.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let mut cache = Self::default();
        for line in text.lines() {
            let fields: Vec<&str> = line.split('\t').collect();
            let [id, name, seen, textures, signature] = fields[..] else {
                continue;
            };
            let (Some(id), Ok(seen)) = (normal_id(id), seen.parse::<u64>()) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let some = |s: &str| (!s.is_empty()).then(|| s.to_owned());
            cache.see(Head {
                id,
                name: name.to_owned(),
                seen,
                textures: some(textures),
                signature: some(signature),
            });
        }
        cache
    }
}

/// A UUID written as 32 hex digits with or without the four dashes, as lower case with dashes.
#[must_use]
pub fn normal_id(text: &str) -> Option<String> {
    let hex: String = text.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    if text.len() == 36 {
        let dashes: Vec<usize> = text.match_indices('-').map(|(i, _)| i).collect();
        if dashes != [8, 13, 18, 23] {
            return None;
        }
    } else if text.len() != 32 {
        return None;
    }
    let hex = hex.to_ascii_lowercase();
    Some(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEVE: &str = "8667ba71-b85a-4004-af54-457a9734eed7";

    fn head(id: &str, name: &str, seen: u64, textures: Option<&str>) -> Head {
        Head {
            id: id.to_owned(),
            name: name.to_owned(),
            seen,
            textures: textures.map(str::to_owned),
            signature: textures.map(|_| "c2ln".to_owned()),
        }
    }

    #[test]
    fn a_player_is_found_by_uuid_or_name() {
        let mut cache = HeadCache::default();
        assert!(cache.see(head(STEVE, "Steve", 10, Some("dGV4"))));
        assert_eq!(cache.find("Steve").unwrap().id, STEVE);
        assert_eq!(cache.find("steve").unwrap().id, STEVE);
        assert_eq!(
            cache
                .find(&STEVE.replace('-', "").to_uppercase())
                .unwrap()
                .name,
            "Steve"
        );
        assert!(cache.find("Alex").is_none());
    }

    #[test]
    fn only_a_new_name_or_skin_is_worth_a_write() {
        let mut cache = HeadCache::default();
        assert!(cache.see(head(STEVE, "Steve", 10, Some("dGV4"))));
        assert!(!cache.see(head(STEVE, "Steve", 20, Some("dGV4"))));
        assert_eq!(cache.find("Steve").unwrap().seen, 20);
        assert!(cache.see(head(STEVE, "Steve", 30, Some("bmV3"))));
        assert!(cache.see(head(STEVE, "Steve2", 40, Some("bmV3"))));
        assert!(cache.find("Steve").is_none(), "the old name is gone");
    }

    #[test]
    fn a_name_two_players_had_belongs_to_the_one_seen_last() {
        let mut cache = HeadCache::default();
        cache.see(head(STEVE, "Sam", 10, None));
        cache.see(head(
            "00000000-0000-0000-0000-000000000001",
            "Sam",
            50,
            None,
        ));
        assert_eq!(cache.find("Sam").unwrap().seen, 50);
    }

    #[test]
    fn the_file_keeps_every_field_and_skips_broken_lines() {
        let mut cache = HeadCache::default();
        cache.see(head(STEVE, "Steve", 10, Some("dGV4")));
        cache.see(head(
            "00000000-0000-0000-0000-000000000001",
            "Cracked",
            11,
            None,
        ));
        let text = cache.to_text();
        let read = HeadCache::from_text(&format!("{text}broken line\nx\ty\tz\t\t\n"));
        assert_eq!(read.len(), 2);
        assert_eq!(read.find("Steve"), cache.find("Steve"));
        assert_eq!(read.find("Cracked").unwrap().textures, None);
    }

    #[test]
    fn the_cache_keeps_the_players_seen_last() {
        let mut cache = HeadCache::default();
        for i in 0..=MAX_ENTRIES as u64 {
            cache.see(head(
                &format!("00000000-0000-0000-0000-{i:012x}"),
                &format!("p{i}"),
                i,
                None,
            ));
        }
        assert_eq!(cache.len(), MAX_ENTRIES);
        assert!(cache.find("p0").is_none());
        assert!(cache.find(&format!("p{MAX_ENTRIES}")).is_some());
    }

    #[test]
    fn uuids_are_read_in_both_forms_only() {
        assert_eq!(normal_id(STEVE).as_deref(), Some(STEVE));
        assert_eq!(
            normal_id("8667BA71B85A4004AF54457A9734EED7").as_deref(),
            Some(STEVE)
        );
        assert!(normal_id("8667ba71-b85a4004-af54-457a9734eed7-").is_none());
        assert!(normal_id("Steve").is_none());
        assert!(normal_id("8667ba71b85a4004af54457a9734eedz").is_none());
    }
}
