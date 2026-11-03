//! Separate versioned marker contracts. Parsed values alone grant no admission.
//! A parsed v2 is inert data: it cannot publish, activate, complete or admit Ready.
use super::{Error, IMPORT_FORMAT, LIMIT, Marker, Outcome, parse_v1_marker};
use crate::open_import::ContentProof;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq)]
struct RestoreV1(Marker);
#[derive(Debug, PartialEq, Eq)]
struct ImportV1(Marker);
#[derive(Debug, PartialEq, Eq)]
enum Contract {
    RestoreV1(RestoreV1),
    ImportV1(ImportV1),
    ImportV2(ImportV2),
    RestoreV2(Marker),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Target {
    New,
    Old,
}
impl<'de> Deserialize<'de> for Target {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Require the wire contract's string, never Serde's enum object form.
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "new" => Ok(Self::New),
            "old" => Ok(Self::Old),
            _ => Err(serde::de::Error::unknown_variant(&value, &["new", "old"])),
        }
    }
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContentContract {
    format: String,
    version: u32,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    sha256: String,
    entry_count: u64,
}
// Distinct role types prevent accidentally passing a replacement as the old
// proof (or vice versa) at construction. Neither type grants verification authority.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
struct PreviousContent(Proof);
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
struct ReplacementContent(Proof);
fn import_proof(value: ContentProof) -> Outcome<Proof> {
    if value.version != 1 || !hash(&value.sha256) {
        return Err(Error::Binding);
    }
    // Reuse the existing proof verbatim; do not recompute, normalize or convert
    // it into the strict restore fingerprint. Count remains an unsigned u64.
    Ok(Proof {
        sha256: value.sha256,
        entry_count: value.entry_count,
    })
}
impl TryFrom<ContentProof> for PreviousContent {
    type Error = Error;
    fn try_from(value: ContentProof) -> Outcome<Self> {
        import_proof(value).map(Self)
    }
}
impl TryFrom<ContentProof> for ReplacementContent {
    type Error = Error;
    fn try_from(value: ContentProof) -> Outcome<Self> {
        import_proof(value).map(Self)
    }
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportV2 {
    format: String,
    version: u32,
    operation_id: String,
    schema_version: u32,
    target: Target,
    #[serde(deserialize_with = "object")]
    content_contract: ContentContract,
    #[serde(deserialize_with = "object")]
    old_content: PreviousContent,
    #[serde(deserialize_with = "object")]
    new_content: ReplacementContent,
    protection_file: String,
    protection_sha256: String,
    candidate_file: String,
    previous_candidate_file: String,
}
// Serde's derived structs also accept positional sequences. The v2 wire
// contract requires named JSON objects, including all three nested objects.
fn object<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Object<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Object<T> {
        type Value = T;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a named JSON object")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<T, M::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(Object(std::marker::PhantomData))
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl ImportV2 {
    fn new(
        operation_id: String,
        target: Target,
        old_content: PreviousContent,
        new_content: ReplacementContent,
        protection_sha256: String,
    ) -> Outcome<Self> {
        Self::new_version(
            operation_id,
            target,
            old_content,
            new_content,
            protection_sha256,
            1,
        )
    }
    fn new_version(
        operation_id: String,
        target: Target,
        old_content: PreviousContent,
        new_content: ReplacementContent,
        protection_sha256: String,
        proof_version: u32,
    ) -> Outcome<Self> {
        let marker = Self {
            format: IMPORT_FORMAT.into(),
            version: if proof_version == 1 { 2 } else { 3 },
            schema_version: if proof_version == 1 { 0 } else { 1 },
            target,
            content_contract: ContentContract {
                format: if proof_version == 1 {
                    "mno-open-import-content"
                } else {
                    "mno-preserved-content"
                }
                .into(),
                version: proof_version,
            },
            protection_file: format!("pre-import-{operation_id}.mno-backup"),
            candidate_file: format!("next-{operation_id}.sqlite"),
            previous_candidate_file: format!("previous-{operation_id}.sqlite"),
            operation_id,
            old_content,
            new_content,
            protection_sha256,
        };
        marker.validate()?;
        Ok(marker)
    }
    fn validate(&self) -> Outcome<()> {
        let id = uuid::Uuid::parse_str(&self.operation_id).map_err(|_| Error::Binding)?;
        if self.format != IMPORT_FORMAT
            || !matches!(
                (
                    self.version,
                    self.schema_version,
                    self.content_contract.format.as_str(),
                    self.content_contract.version
                ),
                (2, 0, "mno-open-import-content", 1) | (3, 1, "mno-preserved-content", 2)
            )
            || id.get_version_num() != 4
            || id.get_variant() != uuid::Variant::RFC4122
            || id.to_string() != self.operation_id
            || !hash(&self.old_content.0.sha256)
            || !hash(&self.new_content.0.sha256)
            || !hash(&self.protection_sha256)
            || self.protection_file != format!("pre-import-{id}.mno-backup")
            || self.candidate_file != format!("next-{id}.sqlite")
            || self.previous_candidate_file != format!("previous-{id}.sqlite")
        {
            return Err(Error::Binding);
        }
        Ok(())
    }
}
fn parse(bytes: &[u8]) -> Outcome<Contract> {
    if bytes.len() as u64 > LIMIT {
        return Err(Error::Binding);
    }
    // Only select a static contract here. Parsing the COMPLETE document again
    // with its exact struct rejects duplicate/extra fields at every object level.
    // Do not dispatch through Value: that would erase duplicate JSON keys.
    #[derive(Deserialize)]
    struct Header {
        format: String,
        version: u32,
    }
    let header: Header = serde_json::from_slice(bytes).map_err(|_| Error::Binding)?;
    match (header.format.as_str(), header.version) {
        ("mno-database-activation", 1) => parse_v1_marker(bytes)
            .map(RestoreV1)
            .map(Contract::RestoreV1),
        ("mno-database-activation", 2) => super::parse_marker(bytes).map(Contract::RestoreV2),
        (IMPORT_FORMAT, 1) => parse_v1_marker(bytes).map(ImportV1).map(Contract::ImportV1),
        (IMPORT_FORMAT, 2 | 3) => {
            let marker: ImportV2 = serde_json::from_slice(bytes).map_err(|_| Error::Binding)?;
            marker.validate()?;
            Ok(Contract::ImportV2(marker))
        }
        _ => Err(Error::Binding),
    }
}

#[cfg(test)]
mod tests;

// Shared production and test execution; parsing alone grants no authority.
mod classification;

pub(super) use classification::transition::complete_v2;
pub(super) use classification::{recovery_view, select_previous};

pub(super) fn is_v2(bytes: &[u8]) -> Outcome<bool> {
    Ok(matches!(parse(bytes)?, Contract::ImportV2(_)))
}

pub(super) mod preparation;

fn import_proof_v2(value: ContentProof) -> Outcome<Proof> {
    if value.version != 2 || !hash(&value.sha256) {
        return Err(Error::Binding);
    }
    Ok(Proof {
        sha256: value.sha256,
        entry_count: value.entry_count,
    })
}
