#![allow(dead_code)]

use std::collections::HashMap;

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
use url::Url;
use xxhash_rust::xxh3::xxh3_64;

pub const BROWSE_PAGE_SIZE: usize = 30;
pub const SEARCH_PAGE_SIZE: usize = 30;
pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.36";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiEnvelope<T> {
    #[serde(rename = "_aMetadata")]
    pub metadata: ApiMetadata,
    #[serde(rename = "_aRecords")]
    pub records: Vec<T>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiMetadata {
    #[serde(rename = "_nRecordCount")]
    pub record_count: usize,
    #[serde(rename = "_nPerpage")]
    pub per_page: usize,
    #[serde(rename = "_bIsComplete")]
    pub is_complete: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CharacterCategory {
    #[serde(rename = "_idRow")]
    pub id: u64,
    #[serde(rename = "_sName")]
    pub name: String,
    #[serde(rename = "_nItemCount", default)]
    pub item_count: u64,
    #[serde(rename = "_sIconUrl")]
    pub icon_url: Option<String>,
    #[serde(rename = "_bIsObsolete", default)]
    pub is_obsolete: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SubmissionAuthor {
    #[serde(rename = "_idRow")]
    pub id: u64,
    #[serde(rename = "_sName")]
    pub name: String,
    #[serde(rename = "_sProfileUrl")]
    pub profile_url: String,
    #[serde(rename = "_sAvatarUrl")]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PreviewMedia {
    #[serde(rename = "_aImages", default)]
    pub images: Vec<PreviewImage>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PreviewImage {
    #[serde(rename = "_sBaseUrl")]
    pub base_url: String,
    #[serde(rename = "_sFile")]
    pub file: String,
    #[serde(rename = "_sFile220")]
    pub file_220: Option<String>,
    #[serde(rename = "_sCaption")]
    pub caption: Option<String>,
    #[serde(rename = "_wFile220")]
    pub width_220: Option<u32>,
    #[serde(rename = "_hFile220")]
    pub height_220: Option<u32>,
}

/// The current preview property (`_aPreviewContent`, apiv13+): a single
/// screenshot per record. `_aPreviewMedia` with its image list is the legacy
/// property that older API versions still serve.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PreviewContent {
    #[serde(default)]
    pub screenshot: Option<PreviewImage>,
}

/// GameBanana renders empty PHP maps as `[]`, and preview shapes vary between
/// API versions; any mismatch here must degrade to "no preview", never fail
/// the whole page parse.
fn lenient_preview_content<'de, D>(deserializer: D) -> Result<Option<PreviewContent>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BrowseRecord {
    #[serde(rename = "_idRow")]
    pub id: u64,
    #[serde(rename = "_sName")]
    pub name: String,
    #[serde(rename = "_sProfileUrl")]
    pub profile_url: String,
    #[serde(rename = "_tsDateAdded")]
    pub date_added: i64,
    #[serde(rename = "_tsDateModified")]
    pub date_modified: i64,
    #[serde(rename = "_tsDateUpdated")]
    pub date_updated: Option<i64>,
    #[serde(rename = "_nLikeCount", default)]
    pub like_count: u64,
    #[serde(rename = "_aSubmitter")]
    pub submitter: SubmissionAuthor,
    #[serde(rename = "_aPreviewMedia")]
    pub preview_media: Option<PreviewMedia>,
    #[serde(
        rename = "_aPreviewContent",
        default,
        deserialize_with = "lenient_preview_content"
    )]
    pub preview_content: Option<PreviewContent>,
    #[serde(rename = "_bHasFiles", default)]
    pub has_files: bool,
    #[serde(rename = "_bHasContentRatings", default)]
    pub has_content_ratings: bool,
    #[serde(rename = "_bIsObsolete", default)]
    pub is_obsolete: bool,
    #[serde(rename = "_sVersion")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreditEntry {
    #[serde(rename = "_aUser")]
    pub user: Option<SubmissionAuthor>,
    #[serde(rename = "_sRole")]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModFile {
    #[serde(rename = "_idRow")]
    pub id: u64,
    #[serde(rename = "_sFile")]
    pub file_name: String,
    #[serde(rename = "_nFilesize")]
    pub file_size: u64,
    #[serde(rename = "_tsDateAdded")]
    pub date_added: i64,
    #[serde(rename = "_nDownloadCount", default)]
    pub download_count: u64,
    #[serde(rename = "_sDescription")]
    pub description: Option<String>,
    #[serde(rename = "_sVersion")]
    pub version: Option<String>,
    #[serde(rename = "_sDownloadUrl")]
    pub download_url: Option<String>,
    #[serde(rename = "_bIsArchived", default)]
    pub is_archived: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateRecord {
    #[serde(rename = "_idRow")]
    pub id: u64,
    #[serde(rename = "_sName", default)]
    pub name: String,
    #[serde(rename = "_tsDateModified", default)]
    pub date_modified: i64,
    #[serde(rename = "_tsDateAdded", default)]
    pub date_added: i64,
    #[serde(rename = "_sProfileUrl", default)]
    pub profile_url: String,
    #[serde(rename = "_sText")]
    pub html_text: Option<String>,
    #[serde(rename = "_sVersion")]
    pub version: Option<String>,
    #[serde(rename = "_bIsPrivate", default)]
    pub is_private: bool,
    #[serde(rename = "_bIsTrashed", default)]
    pub is_trashed: bool,
    #[serde(rename = "_bIsWithheld", default)]
    pub is_withheld: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct TrashInfo {
    #[serde(rename = "_bIsTrashedByOwner", default)]
    pub is_trashed_by_owner: bool,
    #[serde(rename = "_aTrasher")]
    pub trasher: Option<SubmissionAuthor>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct WithholdRule {
    #[serde(rename = "_sCode")]
    pub code: Option<String>,
    #[serde(rename = "_sName")]
    pub name: Option<String>,
    #[serde(rename = "_sText")]
    pub text: Option<String>,
    #[serde(rename = "_sFixInstructions")]
    pub fix_instructions: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct WithholdNotice {
    #[serde(rename = "_tsDateWithheld")]
    pub date_withheld: Option<i64>,
    #[serde(rename = "_sType")]
    pub withhold_type: Option<String>,
    #[serde(rename = "_bIsInReview", default)]
    pub is_in_review: bool,
    #[serde(rename = "_bHasFailedReview", default)]
    pub has_failed_review: bool,
    #[serde(rename = "_aRulesViolated", default)]
    pub rules_violated: Vec<WithholdRule>,
    #[serde(rename = "_sNotes")]
    pub notes: Option<String>,
    #[serde(rename = "_aWithholder")]
    pub withholder: Option<SubmissionAuthor>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SubmissionCategory {
    #[serde(rename = "_sName", default)]
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ProfileResponse {
    #[serde(rename = "_idRow", default)]
    pub id: u64,
    #[serde(rename = "_bIsPrivate", default)]
    pub is_private: bool,
    #[serde(rename = "_bIsDeleted", default)]
    pub is_deleted: bool,
    #[serde(rename = "_bIsTrashed", default)]
    pub is_trashed: bool,
    #[serde(rename = "_bIsWithheld", default)]
    pub is_withheld: bool,
    #[serde(rename = "_aTrashInfo")]
    pub trash_info: Option<TrashInfo>,
    #[serde(rename = "_aWithholdNotice")]
    pub withhold_notice: Option<WithholdNotice>,
    #[serde(rename = "_sName", default)]
    pub name: String,
    #[serde(rename = "_sProfileUrl", default)]
    pub profile_url: String,
    #[serde(rename = "_sDescription")]
    pub short_description: Option<String>,
    #[serde(rename = "_sText")]
    pub html_text: Option<String>,
    #[serde(rename = "_nLikeCount", default)]
    pub like_count: u64,
    #[serde(rename = "_nDownloadCount", default)]
    pub download_count: u64,
    #[serde(rename = "_tsDateAdded", default)]
    pub date_added: i64,
    #[serde(rename = "_tsDateModified", default)]
    pub date_modified: i64,
    #[serde(rename = "_tsDateUpdated")]
    pub date_updated: Option<i64>,
    #[serde(rename = "_sDownloadUrl")]
    pub mod_download_url: Option<String>,
    #[serde(rename = "_aPreviewMedia")]
    pub preview_media: Option<PreviewMedia>,
    #[serde(rename = "_aSubmitter")]
    pub submitter: Option<SubmissionAuthor>,
    #[serde(rename = "_aCredits", default)]
    pub credits: Vec<CreditEntry>,
    #[serde(rename = "_aFiles", default)]
    pub files: Vec<ModFile>,
    #[serde(rename = "_aArchivedFiles", default)]
    pub archived_files: Vec<ModFile>,
    #[serde(rename = "_aContentRatings", default)]
    pub content_ratings: HashMap<String, String>,
    #[serde(rename = "_aEmbeddedMedia", default)]
    pub embedded_media: Vec<String>,
    #[serde(rename = "_aCategory")]
    pub category: Option<SubmissionCategory>,
    #[serde(rename = "_aSuperCategory")]
    pub super_category: Option<SubmissionCategory>,
}

pub fn profile_category_name(profile: &ProfileResponse) -> Option<String> {
    let super_name = profile
        .super_category
        .as_ref()
        .map(|category| category.name.trim())
        .filter(|name| !name.is_empty());
    let category_name = profile
        .category
        .as_ref()
        .map(|category| category.name.trim())
        .filter(|name| !name.is_empty());
    match (super_name, category_name) {
        (Some(super_name), Some(category_name)) => Some(format!("{super_name}: {category_name}")),
        (Some(super_name), None) => Some(super_name.to_string()),
        (None, Some(category_name)) => Some(category_name.to_string()),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_thumbnail_prefers_current_preview_content_over_legacy_media() {
        let record: BrowseRecord = serde_json::from_str(
            r#"{
                "_idRow": 1,
                "_sName": "x",
                "_sProfileUrl": "https://gamebanana.com/mods/1",
                "_tsDateAdded": 1,
                "_tsDateModified": 2,
                "_aSubmitter": { "_idRow": 7, "_sName": "author", "_sProfileUrl": "https://gamebanana.com/members/7" },
                "_aPreviewMedia": {
                    "_aImages": [{ "_sBaseUrl": "https://images.gamebanana.com/img/ss/mods", "_sFile": "legacy.jpg", "_sFile220": "legacy_220.webp" }]
                },
                "_aPreviewContent": {
                    "screenshot": { "_sBaseUrl": "https://images.gamebanana.com/img/ss/mods", "_sFile": "current.jpg", "_sFile220": "current_220.webp" }
                }
            }"#,
        )
        .unwrap();
        let image = record_thumbnail_image(&record).expect("some preview");
        assert_eq!(image.file, "current.jpg");
    }

    #[test]
    fn category_index_record_thumbnail_reads_preview_content() {
        // Shape captured from a live apiv13/Mod/Index response: no
        // _aPreviewMedia, single screenshot under _aPreviewContent.
        let record: BrowseRecord = serde_json::from_str(
            r#"{
                "_idRow": 431561,
                "_sName": "Some Mod",
                "_sProfileUrl": "https://gamebanana.com/mods/431561",
                "_tsDateAdded": 1,
                "_tsDateModified": 2,
                "_aSubmitter": { "_idRow": 7, "_sName": "author", "_sProfileUrl": "https://gamebanana.com/members/7" },
                "_aPreviewContent": {
                    "screenshot": {
                        "_sFile220": "sgi_common_thumbs_66bddc0c8d974_220.webp",
                        "_hFile220": 124,
                        "_sBaseUrl": "https://images.gamebanana.com/img/ss/mods",
                        "_sFile": "66bddc0c8d974.jpg"
                    }
                }
            }"#,
        )
        .unwrap();

        let image = record_thumbnail_image(&record).expect("preview content screenshot");
        assert_eq!(
            thumbnail_url(image).as_deref(),
            Some("https://images.gamebanana.com/img/ss/mods/sgi_common_thumbs_66bddc0c8d974_220.webp"),
        );
    }

    #[test]
    fn unexpected_preview_content_shapes_degrade_to_no_thumbnail() {
        // GameBanana renders empty PHP maps as [] — must not fail the page parse.
        let record: BrowseRecord = serde_json::from_str(
            r#"{
                "_idRow": 1,
                "_sName": "x",
                "_sProfileUrl": "https://gamebanana.com/mods/1",
                "_tsDateAdded": 1,
                "_tsDateModified": 2,
                "_aSubmitter": { "_idRow": 7, "_sName": "author", "_sProfileUrl": "https://gamebanana.com/members/7" },
                "_aPreviewContent": []
            }"#,
        )
        .unwrap();
        assert!(record_thumbnail_image(&record).is_none());
    }

    #[test]
    fn profile_category_name_joins_super_and_leaf_categories() {
        let profile: ProfileResponse = serde_json::from_str(
            r#"{
                "_aSuperCategory": { "_sName": "Operators" },
                "_aCategory": { "_sName": "Tangtang" }
            }"#,
        )
        .unwrap();

        assert_eq!(
            profile_category_name(&profile).as_deref(),
            Some("Operators: Tangtang")
        );
    }

    #[test]
    fn profile_category_name_uses_single_available_category() {
        let profile: ProfileResponse =
            serde_json::from_str(r#"{ "_aCategory": { "_sName": "Tangtang" } }"#).unwrap();

        assert_eq!(profile_category_name(&profile).as_deref(), Some("Tangtang"));
    }

    #[test]
    fn profile_category_name_is_none_without_valid_category_metadata() {
        let profile: ProfileResponse = serde_json::from_str(
            r#"{
                "_aSuperCategory": { "_sName": " " },
                "_aCategory": { "_sName": "" }
            }"#,
        )
        .unwrap();

        assert_eq!(profile_category_name(&profile), None);
    }

    #[test]
    fn browse_json_cache_keys_are_deterministic_and_versioned() {
        let first = browse_page_cache_key("genshin", 1, crate::model::BrowseSort::Popular);
        let second = browse_page_cache_key("genshin", 1, crate::model::BrowseSort::Popular);

        assert_eq!(first, second);
        assert!(first.starts_with("gb-json:v2:"));
        assert_eq!(
            search_page_cache_key(
                "genshin",
                "  Furina  ",
                1,
                crate::model::SearchSort::BestMatch
            ),
            search_page_cache_key("genshin", "Furina", 1, crate::model::SearchSort::BestMatch),
        );
    }
}

pub fn trashed_by_owner(profile: &ProfileResponse) -> Option<&SubmissionAuthor> {
    let info = profile.trash_info.as_ref()?;
    if profile.is_trashed && info.is_trashed_by_owner {
        info.trasher.as_ref()
    } else {
        None
    }
}

pub fn withheld_notice(profile: &ProfileResponse) -> Option<&WithholdNotice> {
    if profile.is_withheld {
        profile.withhold_notice.as_ref()
    } else {
        None
    }
}

pub fn is_unavailable(profile: &ProfileResponse) -> bool {
    profile.is_private
        || profile.is_deleted
        || profile.id == 0
        || trashed_by_owner(profile).is_some()
        || withheld_notice(profile).is_some()
}

pub fn install_block_reason(profile: &ProfileResponse) -> Option<String> {
    if profile.is_private {
        Some("This mod is private and cannot be installed automatically.".to_string())
    } else if let Some(trasher) = trashed_by_owner(profile) {
        Some(format!("This mod has been deleted by {}.", trasher.name))
    } else if withheld_notice(profile).is_some() {
        Some("This mod has been withheld and cannot be installed automatically.".to_string())
    } else if profile.is_deleted || profile.id == 0 {
        Some("This mod no longer exists and cannot be installed automatically.".to_string())
    } else {
        None
    }
}

pub fn unavailable_reason(profile: &ProfileResponse) -> Option<String> {
    if profile.is_private {
        Some("Mod is now private".to_string())
    } else if let Some(trasher) = trashed_by_owner(profile) {
        Some(format!("Mod was deleted by {}", trasher.name))
    } else if let Some(notice) = withheld_notice(profile) {
        if let Some(withholder) = notice.withholder.as_ref() {
            Some(format!("Mod was withheld by {}", withholder.name))
        } else {
            Some("Mod is now withheld".to_string())
        }
    } else if profile.is_deleted || profile.id == 0 {
        Some("Mod no longer exists".to_string())
    } else {
        None
    }
}

pub fn game_id_for_hestia(game_id: &str) -> Option<u64> {
    match game_id {
        "endfield" => Some(21842),
        "wuwa" => Some(20357),
        "genshin" => Some(8552),
        "starrail" => Some(18366),
        "honkai-impact" => Some(10349),
        "zzz" => Some(19567),
        "nte" => Some(23012),
        _ => None,
    }
}

pub fn character_super_category_id_for_hestia(game_id: &str) -> Option<u64> {
    match game_id {
        "endfield" => Some(42770),
        "wuwa" => Some(29524),
        "genshin" => Some(18140),
        "starrail" => Some(22832),
        "honkai-impact" => Some(23620),
        "zzz" => Some(30305),
        "nte" => Some(37906),
        _ => None,
    }
}

pub fn fetch_browse_page(
    client: &Client,
    game_id: u64,
    page: usize,
    sort: crate::model::BrowseSort,
) -> Result<ApiEnvelope<BrowseRecord>> {
    let mut url = Url::parse("https://gamebanana.com/apiv11/Mod/Index")?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("_nPerpage", &BROWSE_PAGE_SIZE.to_string());
        query.append_pair("_nPage", &page.to_string());
        query.append_pair("_aFilters[Generic_Game]", &game_id.to_string());
        if sort == crate::model::BrowseSort::Popular {
            query.append_pair("_sSort", "Generic_MostDownloaded");
        }
    }
    client
        .get(url.as_str())
        .send()
        .context("failed to fetch GameBanana browse page")?
        .error_for_status()
        .context("GameBanana browse page returned an error")?
        .json()
        .context("failed to parse GameBanana browse page")
}

pub async fn fetch_browse_page_async(
    client: &ClientWithMiddleware,
    game_id: u64,
    page: usize,
    sort: crate::model::BrowseSort,
    nocache: bool,
) -> Result<ApiEnvelope<BrowseRecord>> {
    let mut url = Url::parse("https://gamebanana.com/apiv11/Mod/Index")?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("_nPerpage", &BROWSE_PAGE_SIZE.to_string());
        query.append_pair("_nPage", &page.to_string());
        query.append_pair("_aFilters[Generic_Game]", &game_id.to_string());
        if sort == crate::model::BrowseSort::Popular {
            query.append_pair("_sSort", "Generic_MostDownloaded");
        }
        if nocache {
            query.append_pair("nocache", "1");
        }
    }
    let response = client
        .get(url.as_str())
        .send()
        .await
        .context("failed to fetch GameBanana browse page")?;
    response
        .error_for_status()
        .context("GameBanana browse page returned an error")?
        .json()
        .await
        .context("failed to parse GameBanana browse page")
}

pub async fn fetch_character_categories_async(
    client: &ClientWithMiddleware,
    super_category_id: u64,
    nocache: bool,
) -> Result<Vec<CharacterCategory>> {
    let mut url = Url::parse("https://gamebanana.com/apiv12/Mod/Categories")?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("_idCategoryRow", &super_category_id.to_string());
        query.append_pair("_sSort", "a_to_z");
        query.append_pair("_bShowEmpty", "true");
        if nocache {
            query.append_pair("nocache", "1");
        }
    }
    let response = client
        .get(url.as_str())
        .send()
        .await
        .context("failed to fetch GameBanana character categories")?;
    response
        .error_for_status()
        .context("GameBanana character categories returned an error")?
        .json()
        .await
        .context("failed to parse GameBanana character categories")
}

pub async fn fetch_character_browse_page_async(
    client: &ClientWithMiddleware,
    category_id: u64,
    query: Option<&str>,
    page: usize,
    sort: crate::model::BrowseSort,
    nocache: bool,
) -> Result<ApiEnvelope<BrowseRecord>> {
    // apiv13 per GameBanana's guidance: `_aPreviewContent` is the current
    // preview property; `_aPreviewMedia` only exists on legacy API versions.
    // Record parsing accepts both shapes (content first, media as fallback).
    let mut url = Url::parse("https://gamebanana.com/apiv13/Mod/Index")?;
    let sort = match sort {
        crate::model::BrowseSort::Popular => "Generic_MostDownloaded",
        crate::model::BrowseSort::RecentUpdated => "Generic_NewAndUpdated",
    };
    {
        let mut query_pairs = url.query_pairs_mut();
        query_pairs.append_pair("_nPerpage", &BROWSE_PAGE_SIZE.to_string());
        query_pairs.append_pair("_nPage", &page.to_string());
        query_pairs.append_pair("_aFilters[Generic_Category]", &category_id.to_string());
        query_pairs.append_pair("_sSort", sort);
        if let Some(q) = query.map(str::trim).filter(|q| !q.is_empty()) {
            query_pairs.append_pair("_aFilters[Generic_Name]", &format!("contains,{}", q));
        }
        if nocache {
            query_pairs.append_pair("nocache", "1");
        }
    }
    let response = client
        .get(url.as_str())
        .send()
        .await
        .context("failed to fetch GameBanana character browse page")?;
    response
        .error_for_status()
        .context("GameBanana character browse page returned an error")?
        .json()
        .await
        .context("failed to parse GameBanana character browse page")
}

pub fn fetch_search_page(
    client: &Client,
    game_id: u64,
    query: &str,
    page: usize,
    sort: crate::model::SearchSort,
) -> Result<ApiEnvelope<BrowseRecord>> {
    let mut url = Url::parse("https://gamebanana.com/apiv11/Util/Search/Results")?;
    let order = match sort {
        crate::model::SearchSort::BestMatch => "best_match",
        crate::model::SearchSort::RecentUpdated => "udate",
    };
    {
        let mut query_pairs = url.query_pairs_mut();
        query_pairs.append_pair("_sModelName", "Mod");
        query_pairs.append_pair("_sOrder", order);
        query_pairs.append_pair("_idGameRow", &game_id.to_string());
        query_pairs.append_pair("_sSearchString", query);
        query_pairs.append_pair(
            "_csvFields",
            "name,description,article,attribs,studio,owner,credits",
        );
        query_pairs.append_pair("_nPerpage", &SEARCH_PAGE_SIZE.to_string());
        query_pairs.append_pair("_nPage", &page.to_string());
    }
    client
        .get(url.as_str())
        .send()
        .context("failed to fetch GameBanana search results")?
        .error_for_status()
        .context("GameBanana search returned an error")?
        .json()
        .context("failed to parse GameBanana search results")
}

pub async fn fetch_search_page_async(
    client: &ClientWithMiddleware,
    game_id: u64,
    query: &str,
    page: usize,
    sort: crate::model::SearchSort,
    nocache: bool,
) -> Result<ApiEnvelope<BrowseRecord>> {
    let mut url = Url::parse("https://gamebanana.com/apiv11/Util/Search/Results")?;
    let order = match sort {
        crate::model::SearchSort::BestMatch => "best_match",
        crate::model::SearchSort::RecentUpdated => "udate",
    };
    {
        let mut query_pairs = url.query_pairs_mut();
        query_pairs.append_pair("_sModelName", "Mod");
        query_pairs.append_pair("_sOrder", order);
        query_pairs.append_pair("_idGameRow", &game_id.to_string());
        query_pairs.append_pair("_sSearchString", query);
        query_pairs.append_pair(
            "_csvFields",
            "name,description,article,attribs,studio,owner,credits",
        );
        query_pairs.append_pair("_nPerpage", &SEARCH_PAGE_SIZE.to_string());
        query_pairs.append_pair("_nPage", &page.to_string());
        if nocache {
            query_pairs.append_pair("nocache", "1");
        }
    }
    let response = client
        .get(url.as_str())
        .send()
        .await
        .context("failed to fetch GameBanana search results")?;
    response
        .error_for_status()
        .context("GameBanana search returned an error")?
        .json()
        .await
        .context("failed to parse GameBanana search results")
}

/// GameBanana hosts tools (e.g. RabbitFX) in a separate API namespace from
/// mods; the numeric ids are not interchangeable between the two, so a tool
/// link must be fetched via `Tool`, never `Mod`.
pub fn is_tool_url(url: &str) -> bool {
    url.contains("/tools/")
}

fn item_api_kind(is_tool: bool) -> &'static str {
    if is_tool { "Tool" } else { "Mod" }
}

pub fn fetch_profile(client: &Client, mod_id: u64) -> Result<ProfileResponse> {
    fetch_profile_typed(client, mod_id, false)
}

pub fn fetch_profile_typed(client: &Client, mod_id: u64, is_tool: bool) -> Result<ProfileResponse> {
    let kind = item_api_kind(is_tool);
    let url = format!("https://gamebanana.com/apiv11/{kind}/{mod_id}/ProfilePage");
    client
        .get(url)
        .send()
        .context("failed to fetch GameBanana mod profile")?
        .error_for_status()
        .context("GameBanana mod profile returned an error")?
        .json()
        .context("failed to parse GameBanana mod profile")
}

pub async fn fetch_profile_async(
    client: &ClientWithMiddleware,
    mod_id: u64,
) -> Result<ProfileResponse> {
    fetch_profile_async_typed(client, mod_id, false).await
}

pub async fn fetch_profile_async_typed(
    client: &ClientWithMiddleware,
    mod_id: u64,
    is_tool: bool,
) -> Result<ProfileResponse> {
    let kind = item_api_kind(is_tool);
    let url = format!("https://gamebanana.com/apiv11/{kind}/{mod_id}/ProfilePage");
    let response = client
        .get(url)
        .send()
        .await
        .context("failed to fetch GameBanana mod profile")?;
    response
        .error_for_status()
        .context("GameBanana mod profile returned an error")?
        .json()
        .await
        .context("failed to parse GameBanana mod profile")
}

pub async fn fetch_updates_async(
    client: &ClientWithMiddleware,
    mod_id: u64,
) -> Result<ApiEnvelope<UpdateRecord>> {
    let mut url = Url::parse(&format!(
        "https://gamebanana.com/apiv11/Mod/{mod_id}/Updates"
    ))?;
    {
        let mut query_pairs = url.query_pairs_mut();
        query_pairs.append_pair("_nPage", "1");
        query_pairs.append_pair("_nPerpage", "50");
    }
    let response = client
        .get(url.as_str())
        .send()
        .await
        .context("failed to fetch GameBanana mod updates")?;
    response
        .error_for_status()
        .context("GameBanana mod updates returned an error")?
        .json()
        .await
        .context("failed to parse GameBanana mod updates")
}

/// Per GameBanana's dev guidance, `_aPreviewContent` is the current preview
/// property (apiv13+) and `_aPreviewMedia` is the legacy one still served by
/// the older endpoints Hestia uses for browse/search; a card thumbnail must
/// consider both, preferring the current shape.
pub fn record_thumbnail_image(record: &BrowseRecord) -> Option<&PreviewImage> {
    record
        .preview_content
        .as_ref()
        .and_then(|content| content.screenshot.as_ref())
        .or_else(|| {
            record
                .preview_media
                .as_ref()
                .and_then(|media| media.images.first())
        })
}

pub fn thumbnail_url(image: &PreviewImage) -> Option<String> {
    Some(format!(
        "{}/{}",
        image.base_url.trim_end_matches('/'),
        image.file_220.as_ref()?
    ))
}

pub fn full_image_url(image: &PreviewImage) -> String {
    format!("{}/{}", image.base_url.trim_end_matches('/'), image.file)
}

pub fn browser_url(mod_id: u64) -> String {
    format!("https://gamebanana.com/mods/{mod_id}")
}

pub fn browser_url_typed(mod_id: u64, is_tool: bool) -> String {
    if is_tool {
        format!("https://gamebanana.com/tools/{mod_id}")
    } else {
        browser_url(mod_id)
    }
}

pub fn all_authors(profile: &ProfileResponse) -> Vec<String> {
    let mut authors = Vec::new();
    if let Some(submitter) = &profile.submitter {
        authors.push(submitter.name.clone());
    }
    for credit in &profile.credits {
        if let Some(user) = &credit.user {
            if !authors
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&user.name))
            {
                authors.push(user.name.clone());
            }
        }
    }
    authors
}
pub fn sanitize_inline(value: &str) -> String {
    value
        .replace("\r\n", " ")
        .replace(['\n', '\r', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn browse_page_cache_key(game_id: &str, page: usize, sort: crate::model::BrowseSort) -> String {
    json_cache_key_v2(&[
        ("kind", "browse".to_string()),
        ("game", game_id.to_string()),
        ("page", page.to_string()),
        ("sort", format!("{sort:?}")),
    ])
}

pub fn character_categories_cache_key(game_id: &str, super_category_id: u64) -> String {
    json_cache_key_v2(&[
        ("kind", "character-categories".to_string()),
        ("game", game_id.to_string()),
        ("super_category", super_category_id.to_string()),
    ])
}

pub fn character_browse_page_cache_key(
    game_id: &str,
    category_id: u64,
    query: Option<&str>,
    page: usize,
    sort: crate::model::BrowseSort,
) -> String {
    json_cache_key_v2(&[
        // Versioned suffix: cached pages are re-serialized BrowseRecords, so
        // entries written before `_aPreviewContent` was parsed have no preview
        // data and must not be reused after the schema/endpoint change.
        ("kind", "character-browse-v13pc".to_string()),
        ("game", game_id.to_string()),
        ("category", category_id.to_string()),
        ("query", query.unwrap_or_default().trim().to_string()),
        ("page", page.to_string()),
        ("sort", format!("{sort:?}")),
    ])
}

pub fn search_page_cache_key(
    game_id: &str,
    query: &str,
    page: usize,
    sort: crate::model::SearchSort,
) -> String {
    json_cache_key_v2(&[
        ("kind", "search".to_string()),
        ("game", game_id.to_string()),
        ("query", query.trim().to_string()),
        ("page", page.to_string()),
        ("sort", format!("{sort:?}")),
    ])
}

pub fn updates_cache_key(mod_id: u64) -> String {
    json_cache_key_v2(&[("kind", "updates".to_string()), ("mod", mod_id.to_string())])
}

/// MY MODS caches the update log under its own key (a timestamped envelope, see
/// `CachedModUpdates`) so its 30-minute freshness window can be enforced across
/// restarts. Kept separate from `updates_cache_key` because that payload is a bare
/// `ApiEnvelope` with no fetch timestamp.
pub fn my_mod_updates_cache_key(mod_id: u64) -> String {
    json_cache_key_v2(&[
        ("kind", "mymod-updates".to_string()),
        ("mod", mod_id.to_string()),
    ])
}

/// Disk-cache envelope for MY MODS update logs: the fetched payload plus the unix
/// timestamp it was fetched at, so the loader can honor the freshness window and
/// still fall back to a stale copy when a refresh fails.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CachedModUpdates {
    pub fetched_at: i64,
    pub payload: ApiEnvelope<UpdateRecord>,
}

/// Cache identity must be deterministic. Serializing `HashMap` values made the key depend on
/// randomized iteration order, creating duplicate files instead of replacing cached responses.
fn json_cache_key_v2(tags: &[(&str, String)]) -> String {
    let serialized = serde_json::to_string(tags).expect("cache-key tags must serialize");
    format!("gb-json:v2:{:016x}", xxh3_64(serialized.as_bytes()))
}

pub fn profile_cache_key(mod_id: u64) -> String {
    format!("gb-json:profile:{mod_id}")
}

/// Tool and mod ids live in different namespaces, so a tool profile must not
/// overwrite the cache slot of the mod that happens to share its number.
pub fn profile_cache_key_typed(mod_id: u64, is_tool: bool) -> String {
    if is_tool {
        format!("gb-json:profile-tool:{mod_id}")
    } else {
        profile_cache_key(mod_id)
    }
}
