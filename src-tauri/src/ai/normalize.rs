use std::collections::{HashMap, HashSet};

use regex::Regex;

use crate::crypto::LocalCipher;

use super::{
    contract::sha256_hex,
    types::{CanonicalDocument, CanonicalMessage, SpanMap},
};

pub const MAX_DOCUMENT_BYTES: usize = 24_000;

pub fn canonicalize_thread(
    cipher: &LocalCipher,
    mut messages: Vec<CanonicalMessage>,
    cloud_redaction: bool,
) -> CanonicalDocument {
    messages.sort_by(|left, right| left.occurred_at.cmp(&right.occurred_at));
    let mut segments: Vec<(String, SpanMap, String, String)> = messages
        .into_iter()
        .map(|message| {
            let normalized = normalize_body(&message.body);
            let transformed = if cloud_redaction {
                redact_emails(cipher, &normalized)
            } else {
                normalized
            };
            let header = format!(
                "[message {} person {} at {}]\n",
                message.source_revision_id, message.person_id, message.occurred_at
            );
            let segment = format!("{header}{transformed}\n[/message]\n");
            let start = header.len();
            let end = start + transformed.len();
            (
                segment,
                SpanMap {
                    source_revision_id: message.source_revision_id.clone(),
                    original_start: 0,
                    original_end: message.body.len(),
                    transformed_start: start,
                    transformed_end: end,
                },
                message.source_revision_id,
                message.person_id,
            )
        })
        .collect();

    let total: usize = segments.iter().map(|segment| segment.0.len()).sum();
    let truncated = total > MAX_DOCUMENT_BYTES;
    if truncated && segments.len() > 1 {
        let first = segments.remove(0);
        let mut selected = vec![first];
        let mut used = selected[0].0.len();
        let mut newest = Vec::new();
        for segment in segments.into_iter().rev() {
            if used + segment.0.len() > MAX_DOCUMENT_BYTES {
                continue;
            }
            used += segment.0.len();
            newest.push(segment);
        }
        newest.reverse();
        selected.extend(newest);
        segments = selected;
    }

    let mut text = String::new();
    let mut span_map = Vec::new();
    let mut source_revision_ids = Vec::new();
    let mut person_ids = Vec::new();
    for (segment, mut span, revision_id, person_id) in segments {
        if !text.is_empty() && text.len() + segment.len() > MAX_DOCUMENT_BYTES {
            continue;
        }
        let offset = text.len();
        span.transformed_start += offset;
        span.transformed_end += offset;
        text.push_str(&segment);
        span_map.push(span);
        source_revision_ids.push(revision_id);
        person_ids.push(person_id);
    }
    person_ids.sort();
    person_ids.dedup();
    let document_hash = sha256_hex(text.as_bytes());
    CanonicalDocument {
        text,
        document_hash,
        truncated,
        span_map,
        source_revision_ids,
        person_ids,
    }
}

pub fn redact_known_aliases(
    _cipher: &LocalCipher,
    text: &str,
    aliases: &HashMap<String, String>,
) -> String {
    let mut aliases: Vec<_> = aliases.iter().collect();
    aliases.sort_by_key(|(alias, _)| std::cmp::Reverse(alias.len()));
    aliases
        .into_iter()
        .fold(text.to_owned(), |output, (alias, person_id)| {
            if alias.trim().is_empty() {
                output
            } else {
                let token = format!("[person:{person_id}]");
                Regex::new(&format!("(?i){}", regex::escape(alias)))
                    .map(|pattern| pattern.replace_all(&output, token.as_str()).into_owned())
                    .unwrap_or(output)
            }
        })
}

fn normalize_body(body: &str) -> String {
    let line_endings_normalized = body.replace("\r\n", "\n").replace('\r', "\n");
    let mut kept = Vec::new();
    for line in line_endings_normalized.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('>')
            || trimmed.starts_with("On ") && trimmed.ends_with(" wrote:")
            || trimmed == "--"
            || trimmed == "-- "
            || trimmed.eq_ignore_ascii_case("sent from my iphone")
            || trimmed.eq_ignore_ascii_case("sent from my android")
        {
            if trimmed == "--" || trimmed == "-- " {
                break;
            }
            continue;
        }
        kept.push(line.trim_end());
    }
    while kept.last().is_some_and(|line| line.trim().is_empty()) {
        kept.pop();
    }
    kept.join("\n").trim().to_owned()
}

fn redact_emails(cipher: &LocalCipher, text: &str) -> String {
    let email = Regex::new(r"(?i)\b[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}\b")
        .expect("email redaction regex is valid");
    let mut unknown = HashSet::new();
    email
        .replace_all(text, |captures: &regex::Captures<'_>| {
            let value = captures
                .get(0)
                .expect("whole match")
                .as_str()
                .to_lowercase();
            unknown.insert(value.clone());
            format!(
                "[person:{}]",
                &cipher.pseudonymous_id("cloud-person", &value)[..16]
            )
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(revision: &str, person: &str, body: &str, occurred_at: &str) -> CanonicalMessage {
        CanonicalMessage {
            source_revision_id: revision.to_owned(),
            person_id: person.to_owned(),
            occurred_at: occurred_at.to_owned(),
            body: body.to_owned(),
        }
    }

    #[test]
    fn removes_signatures_quotes_and_redacts_cloud_addresses() {
        let cipher = LocalCipher::random();
        let document = canonicalize_thread(
            &cipher,
            vec![message(
                "r1",
                "p1",
                "Please send it to ada@example.com.\n> old quote\n-- \nAda",
                "2026-08-17T10:00:00Z",
            )],
            true,
        );
        assert!(document.text.contains("Please send it to [person:"));
        assert!(!document.text.contains("ada@example.com"));
        assert!(!document.text.contains("old quote"));
        assert_eq!(document.span_map.len(), 1);
    }

    #[test]
    fn truncation_preserves_first_and_newest_messages() {
        let cipher = LocalCipher::random();
        let long = "x".repeat(12_000);
        let document = canonicalize_thread(
            &cipher,
            vec![
                message("first", "p1", "FIRST", "2026-08-17T01:00:00Z"),
                message("middle", "p2", &long, "2026-08-17T02:00:00Z"),
                message("newest", "p1", "NEWEST", "2026-08-17T03:00:00Z"),
                message("huge", "p2", &long, "2026-08-17T02:30:00Z"),
            ],
            false,
        );
        assert!(document.truncated);
        assert!(document.text.contains("FIRST"));
        assert!(document.text.contains("NEWEST"));
        assert!(document.text.len() <= MAX_DOCUMENT_BYTES);
    }
}
