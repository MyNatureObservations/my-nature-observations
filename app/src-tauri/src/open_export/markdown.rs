//! Reading view only: unsupported structures are never partially rendered.
use super::{Entry, Result};
use serde_json::Value;
use std::io::Write;
const RENDER_LIMIT: usize = 1024 * 1024;
fn keys(v: &Value, allowed: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|m| m.keys().all(|k| allowed.contains(&k.as_str())))
}
fn children(v: &Value) -> &[Value] {
    v.get("content")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}
fn kind(v: &Value) -> &str {
    v.get("type").and_then(Value::as_str).unwrap_or("")
}
fn attrs(v: &Value, allowed: &[&str]) -> bool {
    v.get("attrs").is_none_or(|a| keys(a, allowed))
}
fn marks(v: &Value) -> bool {
    v.get("marks").is_none_or(|m| {
        m.as_array().is_some_and(|a| {
            a.iter().all(|m| {
                if !keys(m, &["type", "attrs"]) {
                    return false;
                }
                match kind(m) {
                    "bold" | "italic" | "strike" | "underline" | "code" => attrs(m, &[]),
                    "link" => {
                        attrs(m, &["href", "target", "rel", "class", "title"])
                            && m.pointer("/attrs/href")
                                .and_then(Value::as_str)
                                .is_some_and(|s| {
                                    ["https://", "http://", "mailto:", "tel:"]
                                        .iter()
                                        .any(|p| s.starts_with(p))
                                })
                            && m.get("attrs")
                                .and_then(Value::as_object)
                                .is_some_and(|a| a.values().all(|v| v.is_null() || v.is_string()))
                    }
                    _ => false,
                }
            })
        })
    })
}
fn known(v: &Value) -> bool {
    if !keys(v, &["type", "content", "text", "attrs", "marks"]) {
        return false;
    }
    if v.get("content").is_some_and(|c| !c.is_array()) || !marks(v) {
        return false;
    }
    if kind(v) != "text" && v.get("text").is_some() {
        return false;
    }
    let c = children(v);
    let inline = |c: &Value| matches!(kind(c), "text" | "hardBreak") && known(c);
    let block = |c: &Value| {
        matches!(
            kind(c),
            "paragraph"
                | "heading"
                | "blockquote"
                | "bulletList"
                | "orderedList"
                | "codeBlock"
                | "horizontalRule"
        ) && known(c)
    };
    match kind(v) {
        "doc" => attrs(v, &[]) && v.get("marks").is_none() && c.iter().all(block),
        "text" => {
            attrs(v, &[])
                && v.get("content").is_none()
                && v.get("text").is_some_and(Value::is_string)
                // Multiline code spans can turn into an unterminated fenced block.
                // Preserve the whole document in the existing original-JSON fallback.
                && !(v["text"].as_str().is_some_and(|text| text.contains(['\n', '\r']))
                    && v.get("marks").and_then(Value::as_array).is_some_and(|marks| {
                        marks.iter().any(|mark| kind(mark) == "code")
                    }))
        }
        "paragraph" => attrs(v, &[]) && c.iter().all(inline),
        "heading" => {
            attrs(v, &["level"])
                && v.pointer("/attrs/level")
                    .is_none_or(|n| n.as_u64().is_some_and(|n| (1..=6).contains(&n)))
                && c.iter().all(inline)
        }
        "hardBreak" | "horizontalRule" => attrs(v, &[]) && c.is_empty(),
        "codeBlock" => {
            attrs(v, &["language"])
                && v.pointer("/attrs/language")
                    .is_none_or(|l| l.is_null() || l.is_string())
                && c.iter()
                    .all(|n| kind(n) == "text" && known(n) && n.get("marks").is_none())
        }
        "blockquote" => attrs(v, &[]) && c.iter().all(block),
        "bulletList" => attrs(v, &[]) && c.iter().all(|n| kind(n) == "listItem" && known(n)),
        "orderedList" => {
            attrs(v, &["start", "type"])
                && v.pointer("/attrs/start").is_none_or(|n| {
                    n.as_u64()
                        .is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n))
                })
                && v.pointer("/attrs/type")
                    .is_none_or(|t| t.is_null() || t.is_string())
                && c.iter().all(|n| kind(n) == "listItem" && known(n))
        }
        "listItem" => attrs(v, &[]) && c.iter().all(block),
        _ => false,
    }
}
fn escaped(w: &mut impl Write, s: &str) -> std::io::Result<()> {
    for c in s.chars() {
        match c {
            '<' => w.write_all(b"&lt;")?,
            '>' => w.write_all(b"&gt;")?,
            '&' => w.write_all(b"&amp;")?,
            '\\' | '`' | '*' | '_' | '[' | ']' | '(' | ')' | '#' | '!' | '|' | '~' => {
                write!(w, "\\{c}")?
            }
            _ => write!(w, "{c}")?,
        }
    }
    Ok(())
}
fn fence(s: &str) -> String {
    let (mut max, mut run) = (0, 0);
    for b in s.bytes() {
        if b == b'`' {
            run += 1;
            max = max.max(run)
        } else {
            run = 0
        }
    }
    "`".repeat((max + 1).max(3))
}
fn inline(w: &mut impl Write, v: &Value) -> std::io::Result<()> {
    if kind(v) == "hardBreak" {
        return w.write_all(b"  \n");
    }
    let text = v["text"].as_str().unwrap_or("");
    let marks = v
        .get("marks")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    // Underline and custom link/list presentation cannot round-trip in Markdown;
    // only the reading view is simplified. JSONL remains authoritative.
    for m in marks {
        match kind(m) {
            "bold" => w.write_all(b"**")?,
            "italic" => w.write_all(b"*")?,
            "strike" => w.write_all(b"~~")?,
            "link" => w.write_all(b"[")?,
            _ => (),
        }
    }
    if marks.iter().any(|m| kind(m) == "code") {
        let f = fence(text);
        write!(w, "{f} {text} {f}")?;
    } else {
        escaped(w, text)?;
    }
    for m in marks.iter().rev() {
        match kind(m) {
            "bold" => w.write_all(b"**")?,
            "italic" => w.write_all(b"*")?,
            "strike" => w.write_all(b"~~")?,
            "link" => {
                w.write_all(b"](")?;
                escaped(
                    w,
                    m.pointer("/attrs/href")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                )?;
                w.write_all(b")")?
            }
            _ => (),
        }
    }
    Ok(())
}
fn blocks(w: &mut impl Write, v: &Value, prefix: &str) -> std::io::Result<()> {
    for n in children(v) {
        match kind(n) {
            "paragraph" | "heading" => {
                write!(w, "{prefix}")?;
                if kind(n) == "heading" {
                    write!(
                        w,
                        "{} ",
                        "#".repeat(
                            n.pointer("/attrs/level")
                                .and_then(Value::as_u64)
                                .unwrap_or(1) as usize
                        )
                    )?;
                }
                for c in children(n) {
                    inline(w, c)?
                }
                w.write_all(b"\n\n")?;
            }
            "horizontalRule" => write!(w, "{prefix}---\n\n")?,
            "blockquote" => blocks(w, n, &format!("{prefix}> "))?,
            "bulletList" | "orderedList" => {
                let start = n
                    .pointer("/attrs/start")
                    .and_then(Value::as_u64)
                    .unwrap_or(1);
                for (i, item) in children(n).iter().enumerate() {
                    let marker = if kind(n) == "orderedList" {
                        format!("{}. ", start + i as u64)
                    } else {
                        "- ".into()
                    };
                    // Put the list marker on its own line, indent child blocks.
                    writeln!(w, "{prefix}{marker}")?;
                    blocks(w, item, &format!("{prefix}    "))?;
                }
            }
            "codeBlock" => {
                let text = children(n)
                    .iter()
                    .filter_map(|c| c["text"].as_str())
                    .collect::<String>();
                let f = fence(&text);
                writeln!(w, "{prefix}{f}")?;
                for line in text.split('\n') {
                    writeln!(w, "{prefix}{line}")?;
                }
                writeln!(w, "{prefix}{f}\n")?;
            }
            _ => unreachable!("checked reading structure"),
        }
    }
    Ok(())
}
pub(super) fn entry(w: &mut impl Write, e: &Entry<'_>, duplicates: bool) -> Result<()> {
    w.write_all(b"## ")?;
    escaped(w, e.entry_date)?;
    if !e.title.is_empty() {
        w.write_all(" — ".as_bytes())?;
        escaped(w, e.title)?;
    }
    w.write_all(b"\n\nEntry ID: ")?;
    escaped(w, e.id)?;
    w.write_all(b"\n\n")?;
    if !duplicates && e.content_json.len() <= RENDER_LIMIT {
        if let Ok(v) = serde_json::from_str::<Value>(e.content_json) {
            if kind(&v) == "doc" && known(&v) {
                blocks(w, &v, "")?;
                return Ok(());
            }
        }
    }
    let f = fence(e.content_json);
    writeln!(
        w,
        "Original JSON / Original-JSON (fallback/reservrepresentation; unsupported, ambiguous or large document):\n\n{f}json\n{}\n{f}\n",
        e.content_json
    )?;
    Ok(())
}

pub(super) fn weather(
    w: &mut impl Write,
    weather: &crate::weather::storage::WeatherRecord,
) -> Result<()> {
    let json = serde_json::to_string_pretty(weather)
        .map_err(|e| super::Error::Io(std::io::Error::other(e)))?;
    let f = fence(&json);
    writeln!(
        w,
        "\nWeather snapshots / Väderuppgifter (stored metadata):\n\n{f}json\n{json}\n{f}\n"
    )?;
    Ok(())
}
