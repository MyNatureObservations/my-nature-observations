//! Strict generation selection record and isolated layout contract.
use serde::{Deserialize, Serialize};
use std::io;
use uuid::Uuid;

const FORMAT: &str = "mno-generation-selection";
pub(super) const LIMIT: usize = 4096;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Phase {
    Pending,
    Admitted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    format: String,
    version: u32,
    pub(super) generation: String,
    pub(super) phase: Phase,
    pub(super) candidate_sha256: String,
}

pub(super) fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid or ambiguous generation selection",
    )
}

impl Record {
    #[cfg(test)]
    pub(super) fn new(generation: Uuid, phase: Phase, digest: String) -> Self {
        Self {
            format: FORMAT.into(),
            version: 1,
            generation: generation.to_string(),
            phase,
            candidate_sha256: digest,
        }
    }

    pub(super) fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() > LIMIT {
            return Err(invalid());
        }
        let record: Self = serde_json::from_slice(bytes).map_err(io::Error::other)?;
        if record.format != FORMAT
            || record.version != 1
            || !Uuid::parse_str(&record.generation)
                .is_ok_and(|id| id.get_version_num() == 4 && id.to_string() == record.generation)
            || record.candidate_sha256.len() != 64
            || !record
                .candidate_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid());
        }
        Ok(record)
    }

    #[cfg(test)]
    pub(super) fn encode(&self) -> io::Result<Vec<u8>> {
        let bytes = serde_json::to_vec(self).map_err(io::Error::other)?;
        Self::decode(&bytes)?;
        Ok(bytes)
    }

    #[cfg(all(windows, test))]
    pub(super) fn follows(&self, previous: Option<&Self>) -> bool {
        match (self.phase, previous) {
            (Phase::Pending, None) => true,
            (Phase::Pending, Some(old)) => {
                old.phase == Phase::Admitted && old.generation != self.generation
            }
            (Phase::Admitted, Some(old)) => {
                old.phase == Phase::Pending
                    && old.generation == self.generation
                    && old.candidate_sha256 == self.candidate_sha256
            }
            _ => false,
        }
    }
}

// Supplied by the owning startup protocol in a future stage. A missing
// record cannot prove freshness, legacy establishment or absence of history.
#[cfg(test)]
#[derive(Clone, Copy)]
pub(super) enum LayoutEvidence {
    Fresh,
    Legacy,
    Managed,
    Uncertain,
}

#[cfg(test)]
#[derive(Debug, Eq, PartialEq)]
pub(super) enum Selection {
    Fresh,
    Legacy,
    Managed(Record),
}

#[cfg(test)]
pub(super) fn classify(
    evidence: LayoutEvidence,
    record: Option<&[u8]>,
    interrupted: bool,
) -> io::Result<Selection> {
    if interrupted {
        return Err(invalid());
    }
    match (evidence, record) {
        (LayoutEvidence::Fresh, None) => Ok(Selection::Fresh),
        (LayoutEvidence::Legacy, None) => Ok(Selection::Legacy),
        (LayoutEvidence::Managed, Some(bytes)) => Ok(Selection::Managed(Record::decode(bytes)?)),
        _ => Err(invalid()),
    }
}

#[cfg(test)]
#[test]
fn strict_codec_and_layout_classification() {
    use LayoutEvidence::*;
    for phase in [Phase::Pending, Phase::Admitted] {
        let record = Record::new(Uuid::new_v4(), phase, "a".repeat(64));
        let bytes = record.encode().unwrap();
        assert_eq!(Record::decode(&bytes).unwrap(), record);
        assert_eq!(
            classify(Managed, Some(&bytes), false).unwrap(),
            Selection::Managed(record)
        );
        let json = String::from_utf8(bytes.clone()).unwrap();
        for malformed in [
            json.replace("\"version\":1", "\"version\":2"),
            json.replace("\"version\":1", "\"version\":1,\"version\":1"),
            json.replace("\"version\":1", "\"version\":1,\"extra\":true"),
            json.replace(&"a".repeat(64), "bad"),
            json.replace("mno-generation-selection", "unknown"),
            json[..json.len() - 1].into(),
        ] {
            assert!(classify(Managed, Some(malformed.as_bytes()), false).is_err());
        }
        assert!(classify(Managed, Some(&bytes), true).is_err());
        assert!(classify(Legacy, Some(&bytes), false).is_err());
    }
    assert!(Record::decode(&vec![b' '; LIMIT + 1]).is_err());
    assert_eq!(classify(Fresh, None, false).unwrap(), Selection::Fresh);
    assert_eq!(classify(Legacy, None, false).unwrap(), Selection::Legacy);
    for evidence in [Managed, Uncertain] {
        assert!(classify(evidence, None, false).is_err());
    }
}
