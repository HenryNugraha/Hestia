#![allow(dead_code)]

use std::{collections::HashMap, time::Duration};

use anyhow::{Context, Result, anyhow};
use once_cell::sync::Lazy;
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
        _ => None,
    }
}

pub fn fetch_browse_page(
    game_id: u64,
    page: usize,
    sort: crate::model::BrowseSort,
) -> Result<ApiEnvelope<BrowseRecord>> {
    let client = client()?;
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
) -> Result<Vec<CharacterCategory>> {
    let mut url = Url::parse("https://gamebanana.com/apiv12/Mod/Categories")?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("_idCategoryRow", &super_category_id.to_string());
        query.append_pair("_sSort", "a_to_z");
        query.append_pair("_bShowEmpty", "true");
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
) -> Result<ApiEnvelope<BrowseRecord>> {
    let mut url = Url::parse("https://gamebanana.com/apiv12/Mod/Index")?;
    let sort = match sort {
        crate::model::BrowseSort::Popular => "Generic_MostDownloaded",
        crate::model::BrowseSort::RecentUpdated => "Generic_Newest",
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
    game_id: u64,
    query: &str,
    page: usize,
    sort: crate::model::SearchSort,
) -> Result<ApiEnvelope<BrowseRecord>> {
    let client = client()?;
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
        query_pairs.append_pair("_csvFields", "name,description,article,attribs,studio,owner,credits");
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
        query_pairs.append_pair("_csvFields", "name,description,article,attribs,studio,owner,credits");
        query_pairs.append_pair("_nPerpage", &SEARCH_PAGE_SIZE.to_string());
        query_pairs.append_pair("_nPage", &page.to_string());
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

pub fn fetch_profile(mod_id: u64) -> Result<ProfileResponse> {
    let client = client()?;
    let url = format!("https://gamebanana.com/apiv11/Mod/{mod_id}/ProfilePage");
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
    let url = format!("https://gamebanana.com/apiv11/Mod/{mod_id}/ProfilePage");
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
    let mut url = Url::parse(&format!("https://gamebanana.com/apiv11/Mod/{mod_id}/Updates"))?;
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

fn client() -> Result<&'static Client> {
    static CLIENT: Lazy<Option<Client>> = Lazy::new(|| {
        Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()
            .ok()
    });
    CLIENT
        .as_ref()
        .ok_or_else(|| anyhow!("failed to initialize shared gamebanana client"))
}

pub fn browse_page_cache_key(game_id: &str, page: usize, sort: crate::model::BrowseSort) -> String {
    let mut tags = HashMap::with_capacity(4);
    tags.insert("kind", "browse".to_string());
    tags.insert("game", game_id.to_string());
    tags.insert("page", page.to_string());
    tags.insert("sort", format!("{sort:?}"));
    let serialized = serde_json::to_string(&tags).unwrap_or_default();
    format!("gb-json:{:016x}", xxh3_64(serialized.as_bytes()))
}

pub fn character_categories_cache_key(game_id: &str, super_category_id: u64) -> String {
    let mut tags = HashMap::with_capacity(3);
    tags.insert("kind", "character-categories".to_string());
    tags.insert("game", game_id.to_string());
    tags.insert("super_category", super_category_id.to_string());
    let serialized = serde_json::to_string(&tags).unwrap_or_default();
    format!("gb-json:{:016x}", xxh3_64(serialized.as_bytes()))
}

pub fn character_browse_page_cache_key(
    game_id: &str,
    category_id: u64,
    query: Option<&str>,
    page: usize,
    sort: crate::model::BrowseSort,
) -> String {
    let mut tags = HashMap::with_capacity(6);
    tags.insert("kind", "character-browse".to_string());
    tags.insert("game", game_id.to_string());
    tags.insert("category", category_id.to_string());
    tags.insert("query", query.unwrap_or_default().trim().to_string());
    tags.insert("page", page.to_string());
    tags.insert("sort", format!("{sort:?}"));
    let serialized = serde_json::to_string(&tags).unwrap_or_default();
    format!("gb-json:{:016x}", xxh3_64(serialized.as_bytes()))
}

pub fn search_page_cache_key(
    game_id: &str,
    query: &str,
    page: usize,
    sort: crate::model::SearchSort,
) -> String {
    let mut tags = HashMap::with_capacity(5);
    tags.insert("kind", "search".to_string());
    tags.insert("game", game_id.to_string());
    tags.insert("query", query.trim().to_string());
    tags.insert("page", page.to_string());
    tags.insert("sort", format!("{sort:?}"));
    let serialized = serde_json::to_string(&tags).unwrap_or_default();
    format!("gb-json:{:016x}", xxh3_64(serialized.as_bytes()))
}

pub fn updates_cache_key(mod_id: u64) -> String {
    let mut tags = HashMap::with_capacity(2);
    tags.insert("kind", "updates".to_string());
    tags.insert("mod", mod_id.to_string());
    let serialized = serde_json::to_string(&tags).unwrap_or_default();
    format!("gb-json:{:016x}", xxh3_64(serialized.as_bytes()))
}

pub fn profile_cache_key(mod_id: u64) -> String {
    format!("gb-json:profile:{mod_id}")
}
