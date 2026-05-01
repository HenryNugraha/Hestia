#![allow(dead_code)]

use std::{collections::HashMap, time::Duration};

use anyhow::{Context, Result, anyhow};
use once_cell::sync::Lazy;
use reqwest::blocking::Client;
use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
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

pub fn fetch_browse_page(
    game_id: u64,
    page: usize,
    sort: crate::model::BrowseSort,
) -> Result<ApiEnvelope<BrowseRecord>> {
    let client = client()?;
    let url = "https://gamebanana.com/apiv11/Mod/Index";
    let mut queries = vec![
        ("_nPerpage", BROWSE_PAGE_SIZE.to_string()),
        ("_nPage", page.to_string()),
        ("_aFilters[Generic_Game]", game_id.to_string()),
    ];
    if sort == crate::model::BrowseSort::Popular {
        queries.push(("_sSort", "Generic_MostDownloaded".to_string()));
    }
    client
        .get(url)
        .query(&queries)
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
    let url = "https://gamebanana.com/apiv11/Mod/Index";
    let mut queries = vec![
        ("_nPerpage", BROWSE_PAGE_SIZE.to_string()),
        ("_nPage", page.to_string()),
        ("_aFilters[Generic_Game]", game_id.to_string()),
    ];
    if sort == crate::model::BrowseSort::Popular {
        queries.push(("_sSort", "Generic_MostDownloaded".to_string()));
    }
    let response = client
        .get(url)
        .query(&queries)
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

pub fn fetch_search_page(
    game_id: u64,
    query: &str,
    page: usize,
    sort: crate::model::SearchSort,
) -> Result<ApiEnvelope<BrowseRecord>> {
    let client = client()?;
    let url = "https://gamebanana.com/apiv11/Util/Search/Results";
    let order = match sort {
        crate::model::SearchSort::BestMatch => "best_match",
        crate::model::SearchSort::RecentUpdated => "udate",
    };
    client
        .get(url)
        .query(&[
            ("_sModelName", "Mod".to_string()),
            ("_sOrder", order.to_string()),
            ("_idGameRow", game_id.to_string()),
            ("_sSearchString", query.to_string()),
            (
                "_csvFields",
                "name,description,article,attribs,studio,owner,credits".to_string(),
            ),
            ("_nPerpage", SEARCH_PAGE_SIZE.to_string()),
            ("_nPage", page.to_string()),
        ])
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
    let url = "https://gamebanana.com/apiv11/Util/Search/Results";
    let order = match sort {
        crate::model::SearchSort::BestMatch => "best_match",
        crate::model::SearchSort::RecentUpdated => "udate",
    };
    let response = client
        .get(url)
        .query(&[
            ("_sModelName", "Mod".to_string()),
            ("_sOrder", order.to_string()),
            ("_idGameRow", game_id.to_string()),
            ("_sSearchString", query.to_string()),
            (
                "_csvFields",
                "name,description,article,attribs,studio,owner,credits".to_string(),
            ),
            ("_nPerpage", SEARCH_PAGE_SIZE.to_string()),
            ("_nPage", page.to_string()),
        ])
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
    let url = format!("https://gamebanana.com/apiv11/Mod/{mod_id}/Updates");
    let response = client
        .get(url)
        .query(&[("_nPage", "1".to_string()), ("_nPerpage", "50".to_string())])
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
    let mut tags = HashMap::new();
    tags.insert("kind", "browse".to_string());
    tags.insert("game", game_id.to_string());
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
    let mut tags = HashMap::new();
    tags.insert("kind", "search".to_string());
    tags.insert("game", game_id.to_string());
    tags.insert("query", query.trim().to_string());
    tags.insert("page", page.to_string());
    tags.insert("sort", format!("{sort:?}"));
    let serialized = serde_json::to_string(&tags).unwrap_or_default();
    format!("gb-json:{:016x}", xxh3_64(serialized.as_bytes()))
}

pub fn updates_cache_key(mod_id: u64) -> String {
    let mut tags = HashMap::new();
    tags.insert("kind", "updates".to_string());
    tags.insert("mod", mod_id.to_string());
    let serialized = serde_json::to_string(&tags).unwrap_or_default();
    format!("gb-json:{:016x}", xxh3_64(serialized.as_bytes()))
}

pub fn profile_cache_key(mod_id: u64) -> String {
    format!("gb-json:profile:{mod_id}")
}
