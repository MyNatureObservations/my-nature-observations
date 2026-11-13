//! Immutable owner-approved offline AviList catalog; no network or user data.
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::LazyLock};
pub(crate) const VERSION: &str = "AviList v2026";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Species {
    pub id: String,
    pub english: String,
    pub swedish: Option<String>,
    pub scientific: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referral: Option<String>,
}
static DATA: LazyLock<Vec<Species>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../catalog/avilist-v2026.json"))
        .expect("reviewed bundled species catalog")
});
static INDEX: LazyLock<HashMap<&'static str, &'static Species>> =
    LazyLock::new(|| DISPLAY_DATA.iter().map(|r| (r.id.as_str(), r)).collect());
// Sourced display supplements/referrals; the original DATA and identities stay intact.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NameSupplement {
    id: String,
    scientific: String,
    taxonomy_version: String,
    name: String,
    referrals: Vec<String>,
    source: AliasSource,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AliasSource {
    url: String,
    version: String,
    reviewed: String,
}
static SUPPLEMENTS: LazyLock<Vec<NameSupplement>> = LazyLock::new(|| {
    let aliases: Vec<NameSupplement> =
        serde_json::from_str(include_str!("../catalog/name-supplements.json"))
            .expect("reviewed name supplements");
    for alias in &aliases {
        let species = DATA
            .iter()
            .find(|s| s.id == alias.id)
            .expect("alias catalog identity");
        assert_eq!(alias.scientific, species.scientific);
        assert_eq!(alias.taxonomy_version, VERSION);
        assert!(
            !alias.name.trim().is_empty()
                && alias.source.url.starts_with("https://")
                && !alias.source.version.is_empty()
                && !alias.source.reviewed.is_empty()
        );
    }
    aliases
});
static DISPLAY_DATA: LazyLock<Vec<Species>> = LazyLock::new(|| {
    DATA.iter()
        .map(|s| {
            let mut display = s.clone();
            if let Some(supplement) = SUPPLEMENTS.iter().find(|a| a.id == s.id) {
                display.swedish = Some(supplement.name.clone());
            }
            display
        })
        .collect()
});
pub(crate) fn get(id: &str) -> Option<&'static Species> {
    INDEX.get(id).copied()
}
pub(crate) fn search(query: &str) -> Vec<Species> {
    let q = query.trim().to_lowercase();
    if q.is_empty() || q.len() > 200 {
        return Vec::new();
    }
    // Rank before truncating: e.g. "skata" otherwise disappears behind
    // 186 earlier substring matches in source taxonomy order. Unicode lowercase
    // is used only for matching; bundled display labels stay unchanged.
    let rank = |name: &str| {
        let name = name.to_lowercase();
        if name == q {
            Some(0)
        } else if name.starts_with(&q) {
            Some(1)
        } else if name.contains(&q) {
            Some(2)
        } else {
            None
        }
    };
    let mut matches: Vec<_> = DISPLAY_DATA
        .iter()
        .filter_map(|species| {
            let canonical = [
                Some(species.english.as_str()),
                species.swedish.as_deref(),
                Some(species.scientific.as_str()),
            ]
            .into_iter()
            .flatten()
            .filter_map(rank)
            .min();
            let referral = SUPPLEMENTS
                .iter()
                .filter(|a| a.id == species.id)
                .flat_map(|a| a.referrals.iter())
                .filter_map(|name| rank(name).map(|rank| (rank, name)))
                .min_by_key(|(rank, _)| *rank);
            let best = canonical
                .map(|r| (r, false))
                .into_iter()
                .chain(referral.map(|(r, _)| (r, true)))
                .min()?;
            let mut result = species.clone();
            if best.1 {
                result.referral = referral.map(|(_, name)| name.clone());
            }
            Some((best, result))
        })
        .collect();
    // Stable ordering retains the catalog order among equally relevant results.
    matches.sort_by_key(|(rank, _)| *rank);
    matches
        .into_iter()
        .take(30)
        .map(|(_, species)| species)
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_names_survive_result_limit_in_all_cases_and_languages() {
        for query in [
            "Skata",
            "skata",
            "SKATA",
            " Pica pica ",
            "PICA PICA",
            "pIcA pIcA",
            "Eurasian Magpie",
            "EURASIAN MAGPIE",
            "eurasian magpie",
        ] {
            let hits = search(query);
            assert_eq!(
                hits.first().map(|s| s.id.as_str()),
                Some("avibase-FBBBA943"),
                "query: {query}"
            );
            assert!(hits.len() <= 30);
            let actual = &hits[0];
            assert_eq!(actual.swedish.as_deref(), Some("skata"));
            assert_eq!(actual.english, "Eurasian Magpie");
            assert_eq!(actual.scientific, "Pica pica");
        }
        for (queries, id, original) in [
            (["blåmes", "BLÅMES", "BlÅmEs"], "avibase-9BE53D34", "blåmes"),
            (
                ["ängspiplärka", "ÄNGSPIPLÄRKA", "Ängspiplärka"],
                "avibase-14873249",
                "ängspiplärka",
            ),
            (
                ["grönfink", "GRÖNFINK", "GrÖnFiNk"],
                "avibase-65BD2033",
                "grönfink",
            ),
            (
                ["nötkråka", "NÖTKRÅKA", "NöTkRåKa"],
                "avibase-D9B001DF",
                "nötkråka",
            ),
        ] {
            for query in queries {
                let hits = search(query);
                assert_eq!(hits[0].id, id, "query: {query}");
                assert_eq!(hits[0].swedish.as_deref(), Some(original));
            }
        }
    }
    #[test]
    fn catalog_identity_names_and_missing_swedish_are_preserved() {
        assert_eq!(DATA.len(), 11172);
        assert_eq!(INDEX.len(), 11172);
        assert_eq!(DATA.iter().filter(|s| s.swedish.is_none()).count(), 28);
        assert!(
            DATA.iter()
                .all(|s| !s.english.is_empty() && !s.scientific.is_empty())
        );
        let s = get("avibase-DF1E2A1D").unwrap();
        assert_eq!(s.english, "Ruddy Duck");
        assert!(s.swedish.is_none());
        assert!(search("talgoxe").iter().any(|s| s.english == "Great Tit"));
        assert!(
            search("Great Tit")
                .iter()
                .any(|s| s.swedish.as_deref() == Some("talgoxe"))
        );
    }
}

#[cfg(test)]
mod crow_tests {
    use super::*;
    #[test]
    fn actual_crow_catalog_and_verified_alias_keep_species_and_subspecies_distinct() {
        let s = get("avibase-E4EE9AC4").unwrap();
        assert_eq!(s.scientific, "Corvus corone");
        assert_eq!(s.english, "Carrion Crow");
        assert_eq!(s.swedish.as_deref(), Some("Kråka"));
        assert!(get("avibase-81B32602").is_none());
        assert!(DATA.iter().all(|s| s.scientific != "Corvus cornix"));
        for query in [
            "kråka",
            "Kråka",
            "KRÅKA",
            " Corvus corone ",
            "Carrion Crow",
            "CARRION CROW",
        ] {
            assert_eq!(search(query)[0].id, s.id, "{query}");
        }
        for q in ["gråkråka", "GRÅKRÅKA", "Gråkråka"] {
            let found = search(q);
            assert_eq!(found[0].id, s.id);
            assert_eq!(found[0].referral.as_deref(), Some("gråkråka"));
        }
        assert_eq!(search("kråka")[0].referral, None);
        assert_eq!(get("avibase-E4EE9AC4").unwrap().referral, None);
        assert!(search("Corvus corone cornix").is_empty());
        assert_eq!(DATA.iter().filter(|s| s.swedish.is_none()).count(), 28);
        assert_eq!(SUPPLEMENTS.len(), 1);
        assert_eq!(SUPPLEMENTS[0].name, "Kråka");
        assert_eq!(
            DATA.iter()
                .find(|s| s.id == "avibase-E4EE9AC4")
                .unwrap()
                .swedish,
            None
        );
    }
}
