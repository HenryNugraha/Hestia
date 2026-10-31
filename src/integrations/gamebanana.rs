#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
use url::Url;
use xxhash_rust::xxh3::xxh3_64;

pub const BROWSE_PAGE_SIZE: usize = 30;
pub const SEARCH_PAGE_SIZE: usize = 30;
pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.36";
pub const GAMEBANANA_API_BASE: &str = "https://gamebanana.com/apiv13";

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
/// screenshot for index records and a screenshot list for profile records.
/// `_aPreviewMedia` with its image list is the legacy property.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PreviewContent {
    #[serde(default)]
    pub screenshot: Option<PreviewImage>,
    #[serde(default)]
    pub screenshots: Vec<PreviewImage>,
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

fn deserialize_profile_preview_media<'de, D>(
    deserializer: D,
) -> Result<Option<PreviewMedia>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() || !value.is_object() {
        return Ok(None);
    }

    if value.get("screenshots").is_some() || value.get("screenshot").is_some() {
        let content: PreviewContent =
            serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        let mut images = content.screenshots;
        if let Some(screenshot) = content.screenshot {
            images.insert(0, screenshot);
        }
        return Ok(Some(PreviewMedia { images }));
    }

    serde_json::from_value(value)
        .map(Some)
        .map_err(serde::de::Error::custom)
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
    #[serde(rename = "_aUser", default)]
    pub user: Option<SubmissionAuthor>,
    /// API v13 can return an unlinked credited author as `_sName` instead of
    /// `_aUser`; keep that name without manufacturing a SubmissionAuthor.
    #[serde(rename = "_sName", default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "_sRole", default)]
    pub role: Option<String>,
}

fn non_empty_string(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn submission_author_from_value(value: &serde_json::Value) -> Option<SubmissionAuthor> {
    let author: SubmissionAuthor = serde_json::from_value(value.clone()).ok()?;
    (!author.name.trim().is_empty()).then_some(author)
}

fn credit_entry_from_value(value: &serde_json::Value) -> Option<CreditEntry> {
    let object = value.as_object()?;

    // Cached profiles use the original flat `_aUser` shape. Parse it first so
    // those entries remain readable alongside the grouped v13 shape.
    if object.contains_key("_aUser") {
        let mut entry: CreditEntry = serde_json::from_value(value.clone()).ok()?;
        if entry.user.is_none() {
            entry.name = non_empty_string(object.get("_sName"));
        }
        entry.role = entry
            .role
            .and_then(|role| (!role.trim().is_empty()).then_some(role));
        if entry.user.is_none() {
            entry.name = entry
                .name
                .take()
                .map(|name| name.trim().to_owned())
                .filter(|name| !name.is_empty());
        } else {
            entry.name = None;
        }
        return (entry.user.is_some() || entry.name.is_some() || entry.role.is_some())
            .then_some(entry);
    }

    // Grouped v13 authors use the SubmissionAuthor fields directly. A
    // name-only author cannot be represented by SubmissionAuthor, so retain
    // its name in CreditEntry instead.
    let user = submission_author_from_value(value);
    let name = if user.is_none() {
        non_empty_string(object.get("_sName"))
    } else {
        None
    };
    let role = non_empty_string(object.get("_sRole"));

    (user.is_some() || name.is_some() || role.is_some()).then_some(CreditEntry { user, name, role })
}

fn append_credit_authors(value: &serde_json::Value, credits: &mut Vec<CreditEntry>) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                if let Some(entry) = credit_entry_from_value(value) {
                    credits.push(entry);
                }
            }
        }
        serde_json::Value::Object(object) => {
            // A single author is occasionally emitted as an object rather
            // than a one-element array. Also accept keyed author maps, while
            // treating the group object itself as a group rather than author.
            if object.keys().any(|key| {
                matches!(
                    key.as_str(),
                    "_aUser" | "_idRow" | "_sName" | "_sProfileUrl" | "_sRole"
                )
            }) {
                if let Some(entry) = credit_entry_from_value(value) {
                    credits.push(entry);
                }
            } else {
                for value in object.values() {
                    append_credit_authors(value, credits);
                }
            }
        }
        serde_json::Value::Null => {}
        _ => {}
    }
}

fn append_credit_value(value: &serde_json::Value, credits: &mut Vec<CreditEntry>) {
    let Some(object) = value.as_object() else {
        return;
    };
    if let Some(authors) = object.get("_aAuthors") {
        append_credit_authors(authors, credits);
    } else if let Some(entry) = credit_entry_from_value(value) {
        credits.push(entry);
    }
}

fn deserialize_credits<'de, D>(deserializer: D) -> Result<Vec<CreditEntry>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    let mut credits = Vec::new();
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                append_credit_value(&value, &mut credits);
            }
        }
        serde_json::Value::Object(_) => append_credit_value(&value, &mut credits),
        serde_json::Value::Null => {}
        _ => {}
    }
    Ok(credits)
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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
    /// GameBanana's MD5 of the archive, checked after every download.
    #[serde(rename = "_sMd5Checksum", default, skip_serializing_if = "Option::is_none")]
    pub md5: Option<String>,
}

fn deserialize_optional_status<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Number(number)) => number
            .as_i64()
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom("GameBanana status is not an integer")),
        Some(serde_json::Value::String(status)) => status
            .trim()
            .parse::<i64>()
            .map(Some)
            .map_err(|_| serde::de::Error::custom("GameBanana status is not an integer")),
        Some(_) => Err(serde::de::Error::custom(
            "GameBanana status is neither a number nor a string",
        )),
    }
}

#[derive(Debug, Deserialize)]
struct RawUpdateFile {
    #[serde(flatten)]
    file: ModFile,
    #[serde(rename = "_bHasContents")]
    has_contents: Option<bool>,
    #[serde(rename = "_sAnalysisState")]
    analysis_state: Option<String>,
    #[serde(rename = "_sAnalysisResult")]
    analysis_result: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawUpdateRecord {
    #[serde(rename = "_idRow")]
    id: u64,
    #[serde(rename = "_sName", default)]
    name: String,
    #[serde(rename = "_tsDateModified", default)]
    date_modified: i64,
    #[serde(rename = "_tsDateAdded", default)]
    date_added: i64,
    #[serde(rename = "_sProfileUrl", default)]
    profile_url: String,
    #[serde(rename = "_sText")]
    html_text: Option<String>,
    #[serde(rename = "_sVersion")]
    version: Option<String>,
    #[serde(
        rename = "_nStatus",
        default,
        deserialize_with = "deserialize_optional_status"
    )]
    status: Option<i64>,
    #[serde(rename = "_aFileRowIds", default)]
    file_ids: Option<Vec<u64>>,
    #[serde(rename = "_aFiles", default)]
    files: Option<Vec<RawUpdateFile>>,
    #[serde(rename = "attachments_usable", default)]
    cached_attachments_usable: Option<bool>,
    #[serde(rename = "_bIsPrivate", default)]
    is_private: bool,
    #[serde(rename = "_bIsTrashed", default)]
    is_trashed: bool,
    #[serde(rename = "_bIsWithheld", default)]
    is_withheld: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    /// GameBanana sends this as either the JSON number `0` or the string
    /// `"0"`; nonzero values mean the update is moderated and unusable.
    #[serde(
        rename = "_nStatus",
        default,
        deserialize_with = "deserialize_optional_status"
    )]
    pub status: Option<i64>,
    /// File ids attached to this update. These are optional because older
    /// records and malformed/partial API responses may omit the field.
    #[serde(rename = "_aFileRowIds", default)]
    pub file_ids: Option<Vec<u64>>,
    /// Full file metadata attached to this update, when GameBanana includes it.
    #[serde(rename = "_aFiles", default)]
    pub files: Option<Vec<ModFile>>,
    /// False when an attached file explicitly failed analysis or has no
    /// contents. Missing analysis fields remain neutral.
    #[serde(rename = "attachments_usable", default = "default_attachments_usable")]
    pub attachments_usable: bool,
    #[serde(rename = "_bIsPrivate", default)]
    pub is_private: bool,
    #[serde(rename = "_bIsTrashed", default)]
    pub is_trashed: bool,
    #[serde(rename = "_bIsWithheld", default)]
    pub is_withheld: bool,
}

fn default_attachments_usable() -> bool {
    true
}

impl<'de> Deserialize<'de> for UpdateRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawUpdateRecord::deserialize(deserializer)?;
        let explicit_attachment_failure = raw.files.as_ref().is_some_and(|files| {
            files.iter().any(|file| {
                file.has_contents == Some(false)
                    || file
                        .analysis_state
                        .as_deref()
                        .map(str::trim)
                        .filter(|state| !state.is_empty())
                        .is_some_and(|state| !state.eq_ignore_ascii_case("done"))
                    || file
                        .analysis_result
                        .as_deref()
                        .map(str::trim)
                        .filter(|result| !result.is_empty())
                        .is_some_and(|result| !result.eq_ignore_ascii_case("ok"))
            })
        });
        let derived_attachments_usable = !explicit_attachment_failure;
        let attachments_usable = match raw.cached_attachments_usable {
            // Serialized cache records no longer contain the raw analysis
            // fields, so preserve their computed result unless the current
            // payload explicitly contains a failed analysis state.
            Some(_) if explicit_attachment_failure => false,
            Some(cached) => cached,
            None => derived_attachments_usable,
        };
        let files = raw
            .files
            .map(|files| files.into_iter().map(|file| file.file).collect());
        Ok(Self {
            id: raw.id,
            name: raw.name,
            date_modified: raw.date_modified,
            date_added: raw.date_added,
            profile_url: raw.profile_url,
            html_text: raw.html_text,
            version: raw.version,
            status: raw.status,
            file_ids: raw.file_ids,
            files,
            attachments_usable,
            is_private: raw.is_private,
            is_trashed: raw.is_trashed,
            is_withheld: raw.is_withheld,
        })
    }
}

impl UpdateRecord {
    /// Whether this update is suitable as public release metadata.
    /// Missing status is treated like the legacy public shape; explicit
    /// nonzero status and moderation flags make the record unusable.
    pub fn is_public(&self) -> bool {
        self.status.unwrap_or_default() == 0
            && !self.is_private
            && !self.is_trashed
            && !self.is_withheld
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct UpdateHistory {
    pub records: Vec<UpdateRecord>,
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
    #[serde(rename = "_idRow", default)]
    pub id: u64,
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
    // API v13 moved profile screenshots to `_aPreviewContent.screenshots`.
    // Keep the public field and serialized key stable for existing consumers
    // and cached profiles while accepting both response shapes on input.
    #[serde(
        rename = "_aPreviewMedia",
        alias = "_aPreviewContent",
        default,
        deserialize_with = "deserialize_profile_preview_media"
    )]
    pub preview_media: Option<PreviewMedia>,
    #[serde(rename = "_aSubmitter")]
    pub submitter: Option<SubmissionAuthor>,
    #[serde(
        rename = "_aCredits",
        default,
        deserialize_with = "deserialize_credits"
    )]
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
    /// Hestia-owned cache metadata. This field is absent for API-only
    /// profiles and present only after a complete update-log fetch.
    #[serde(
        rename = "update_history",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub update_history: Option<UpdateHistory>,
    /// Hestia-owned cache metadata: GameBanana's archive path list per file
    /// id, attached only when the update matcher asks for it.
    #[serde(
        rename = "file_listings",
        default,
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub file_listings: BTreeMap<u64, Vec<String>>,
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
            Some(
                "https://images.gamebanana.com/img/ss/mods/sgi_common_thumbs_66bddc0c8d974_220.webp"
            ),
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
    fn profile_v13_screenshots_and_download_fields_survive_deserialization() {
        let profile: ProfileResponse = serde_json::from_str(
            r#"{
                "_aPreviewContent": {
                    "screenshots": [
                        {
                            "_sBaseUrl": "https://images.gamebanana.com/img/ss/mods",
                            "_sFile": "first.jpg",
                            "_sFile220": "first_220.webp"
                        },
                        {
                            "_sBaseUrl": "https://images.gamebanana.com/img/ss/mods",
                            "_sFile": "second.jpg"
                        }
                    ]
                },
                "_aFiles": [
                    {
                        "_idRow": 10,
                        "_sFile": "mod.zip",
                        "_nFilesize": 42,
                        "_tsDateAdded": 1,
                        "_sDownloadUrl": "https://gamebanana.com/dl/10"
                    }
                ]
            }"#,
        )
        .unwrap();

        let preview = profile.preview_media.expect("v13 profile preview");
        assert_eq!(preview.images.len(), 2);
        assert_eq!(preview.images[1].file, "second.jpg");
        assert_eq!(
            profile.files[0].download_url.as_deref(),
            Some("https://gamebanana.com/dl/10")
        );
    }

    #[test]
    fn legacy_profile_preview_cache_shape_round_trips() {
        let profile: ProfileResponse = serde_json::from_str(
            r#"{
                "_aPreviewMedia": {
                    "_aImages": [{
                        "_sBaseUrl": "https://images.gamebanana.com/img/ss/mods",
                        "_sFile": "legacy.jpg"
                    }]
                }
            }"#,
        )
        .unwrap();
        let cached = serde_json::to_value(&profile).unwrap();
        assert!(cached.get("_aPreviewMedia").is_some());
        assert!(cached.get("_aPreviewContent").is_none());

        let restored: ProfileResponse = serde_json::from_value(cached).unwrap();
        assert_eq!(restored.preview_media.unwrap().images[0].file, "legacy.jpg");
    }

    #[test]
    fn profile_v13_grouped_credits_preserve_name_only_and_linked_authors() {
        let profile: ProfileResponse = serde_json::from_str(
            r#"{
                "_aSubmitter": {
                    "_idRow": 1,
                    "_sName": "Submitter",
                    "_sProfileUrl": "https://gamebanana.com/members/1"
                },
                "_aCredits": [
                    {
                        "_sGroupName": "Key Authors",
                        "_aAuthors": [
                            { "_sRole": "Recolor", "_sName": "YouWhenMe" },
                            {
                                "_sRole": "Developer",
                                "_idRow": 2,
                                "_sName": "Dimit",
                                "_sProfileUrl": "https://gamebanana.com/members/2",
                                "_sAvatarUrl": "https://images.gamebanana.com/img/av/2.jpg"
                            }
                        ]
                    }
                ]
            }"#,
        )
        .unwrap();

        assert_eq!(profile.credits.len(), 2);
        assert_eq!(profile.credits[0].name.as_deref(), Some("YouWhenMe"));
        assert!(profile.credits[0].user.is_none());
        assert_eq!(profile.credits[0].role.as_deref(), Some("Recolor"));
        assert_eq!(profile.credits[1].user.as_ref().unwrap().name, "Dimit");
        assert_eq!(
            profile.credits[1].user.as_ref().unwrap().profile_url,
            "https://gamebanana.com/members/2"
        );
        assert_eq!(profile.credits[1].role.as_deref(), Some("Developer"));
        assert_eq!(
            all_authors(&profile),
            vec![
                "Submitter".to_owned(),
                "YouWhenMe".to_owned(),
                "Dimit".to_owned()
            ]
        );

        let cached = serde_json::to_value(&profile).unwrap();
        let restored: ProfileResponse = serde_json::from_value(cached).unwrap();
        assert_eq!(restored.credits.len(), 2);
        assert_eq!(restored.credits[0].name.as_deref(), Some("YouWhenMe"));
        assert_eq!(restored.credits[1].user.as_ref().unwrap().name, "Dimit");
    }

    #[test]
    fn legacy_flat_credits_and_empty_credit_shapes_remain_readable() {
        let profile: ProfileResponse = serde_json::from_str(
            r#"{
                "_aCredits": [
                    {
                        "_aUser": {
                            "_idRow": 2,
                            "_sName": "Dimit",
                            "_sProfileUrl": "https://gamebanana.com/members/2"
                        },
                        "_sRole": "Developer"
                    }
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(profile.credits.len(), 1);
        assert_eq!(profile.credits[0].user.as_ref().unwrap().name, "Dimit");
        assert_eq!(profile.credits[0].role.as_deref(), Some("Developer"));

        let cached = serde_json::to_value(&profile).unwrap();
        let restored: ProfileResponse = serde_json::from_value(cached).unwrap();
        assert_eq!(restored.credits.len(), 1);
        assert_eq!(restored.credits[0].user.as_ref().unwrap().name, "Dimit");

        for value in [
            r#"{ "_aCredits": null }"#,
            r#"{ "_aCredits": [] }"#,
            r#"{ "_aCredits": { "_sGroupName": "Empty", "_aAuthors": [] } }"#,
        ] {
            let profile: ProfileResponse = serde_json::from_str(value).unwrap();
            assert!(profile.credits.is_empty());
        }

        let singleton_group: ProfileResponse = serde_json::from_str(
            r#"{
                "_aCredits": {
                    "_sGroupName": "Original Author",
                    "_aAuthors": {
                        "_sRole": "Developer",
                        "_sName": "Dimit",
                        "_idRow": 2,
                        "_sProfileUrl": "https://gamebanana.com/members/2"
                    }
                }
            }"#,
        )
        .unwrap();
        assert_eq!(singleton_group.credits.len(), 1);
        assert_eq!(
            singleton_group.credits[0].user.as_ref().unwrap().name,
            "Dimit"
        );
    }

    #[test]
    fn all_authors_deduplicates_case_insensitively_and_ignores_empty_names() {
        let profile: ProfileResponse = serde_json::from_str(
            r#"{
                "_aSubmitter": {
                    "_idRow": 1,
                    "_sName": "  Dimit  ",
                    "_sProfileUrl": "https://gamebanana.com/members/1"
                },
                "_aCredits": [
                    { "_sGroupName": "Group label", "_aAuthors": [
                        { "_sName": "dImIt", "_sRole": "Artist" },
                        { "_sName": "  ", "_sRole": "Empty" },
                        { "_sRole": "No name" },
                        { "_sName": "Artist" }
                    ] }
                ]
            }"#,
        )
        .unwrap();

        assert_eq!(
            all_authors(&profile),
            vec!["Dimit".to_owned(), "Artist".to_owned()]
        );
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
    fn moderator_trashed_profile_is_unavailable_without_optional_metadata() {
        let profile: ProfileResponse = serde_json::from_str(
            r#"{
                "_idRow": 418715,
                "_bIsPrivate": false,
                "_bIsDeleted": false,
                "_bIsTrashed": true,
                "_bIsWithheld": false,
                "_aTrashInfo": { "_bIsTrashedByOwner": false },
                "_aFiles": []
            }"#,
        )
        .unwrap();

        assert!(is_unavailable(&profile));
        assert_eq!(
            install_block_reason(&profile).as_deref(),
            Some("This mod has been deleted and cannot be installed automatically.")
        );
        assert_eq!(
            unavailable_reason(&profile).as_deref(),
            Some("Mod was deleted")
        );
    }

    #[test]
    fn owner_trash_and_withheld_attribution_preserve_specific_reasons() {
        let owner_trashed: ProfileResponse = serde_json::from_str(
            r#"{
                "_idRow": 1,
                "_bIsTrashed": true,
                "_aTrashInfo": {
                    "_bIsTrashedByOwner": true,
                    "_aTrasher": {
                        "_idRow": 2,
                        "_sName": "Owner",
                        "_sProfileUrl": "https://gamebanana.com/members/2"
                    }
                }
            }"#,
        )
        .unwrap();
        assert!(is_unavailable(&owner_trashed));
        assert_eq!(
            install_block_reason(&owner_trashed).as_deref(),
            Some("This mod has been deleted by Owner.")
        );
        assert_eq!(
            unavailable_reason(&owner_trashed).as_deref(),
            Some("Mod was deleted by Owner")
        );

        let withheld: ProfileResponse = serde_json::from_str(
            r#"{
                "_idRow": 3,
                "_bIsWithheld": true,
                "_aWithholdNotice": {
                    "_aWithholder": {
                        "_idRow": 4,
                        "_sName": "Moderator",
                        "_sProfileUrl": "https://gamebanana.com/members/4"
                    }
                }
            }"#,
        )
        .unwrap();
        assert!(is_unavailable(&withheld));
        assert_eq!(
            install_block_reason(&withheld).as_deref(),
            Some("This mod has been withheld and cannot be installed automatically.")
        );
        assert_eq!(
            unavailable_reason(&withheld).as_deref(),
            Some("Mod was withheld by Moderator")
        );
    }

    #[test]
    fn withheld_profile_is_unavailable_without_notice_metadata() {
        let profile: ProfileResponse = serde_json::from_str(
            r#"{
                "_idRow": 5,
                "_bIsPrivate": false,
                "_bIsDeleted": false,
                "_bIsTrashed": false,
                "_bIsWithheld": true,
                "_aFiles": []
            }"#,
        )
        .unwrap();

        assert!(is_unavailable(&profile));
        assert_eq!(
            install_block_reason(&profile).as_deref(),
            Some("This mod has been withheld and cannot be installed automatically.")
        );
        assert_eq!(
            unavailable_reason(&profile).as_deref(),
            Some("Mod is now withheld")
        );
    }

    #[test]
    fn available_profile_is_not_blocked_by_optional_moderation_metadata() {
        let profile: ProfileResponse = serde_json::from_str(
            r#"{
                "_idRow": 703505,
                "_bIsPrivate": false,
                "_bIsDeleted": false,
                "_bIsTrashed": false,
                "_bIsWithheld": false,
                "_aTrashInfo": { "_bIsTrashedByOwner": false },
                "_aWithholdNotice": null,
                "_aFiles": []
            }"#,
        )
        .unwrap();

        assert!(!is_unavailable(&profile));
        assert_eq!(install_block_reason(&profile), None);
        assert_eq!(unavailable_reason(&profile), None);
    }

    #[test]
    fn update_records_accept_string_and_numeric_status_and_round_trip_attachments() {
        let profile: ProfileResponse = serde_json::from_str(
            r#"{
                "_idRow": 728107,
                "update_history": {
                    "records": [
                        {
                            "_idRow": 465719,
                            "_nStatus": "0",
                            "_aFileRowIds": [1853590],
                            "_aFiles": [{
                                "_idRow": 1853590,
                                "_sFile": "si101.rar",
                                "_nFilesize": 123,
                                "_tsDateAdded": 456,
                                "_bHasContents": true,
                                "_sAnalysisState": "done",
                                "_sAnalysisResult": "ok",
                                "_sDownloadUrl": "https://gamebanana.com/dl/1853590"
                            }]
                        },
                        {"_idRow": 465720, "_nStatus": 1},
                        {
                            "_idRow": 465721,
                            "_aFileRowIds": [1853591],
                            "_aFiles": [{
                                "_idRow": 1853591,
                                "_sFile": "failed.rar",
                                "_nFilesize": 123,
                                "_tsDateAdded": 456,
                                "_bHasContents": false,
                                "_sAnalysisState": "done",
                                "_sAnalysisResult": "ok"
                            }]
                        },
                        {
                            "_idRow": 465722,
                            "_aFileRowIds": [1853592],
                            "_aFiles": [{
                                "_idRow": 1853592,
                                "_sFile": "unknown-analysis.rar",
                                "_nFilesize": 123,
                                "_tsDateAdded": 456
                            }]
                        }
                    ]
                }
            }"#,
        )
        .unwrap();
        let history = profile.update_history.as_ref().expect("cached history");
        assert_eq!(history.records[0].status, Some(0));
        assert!(history.records[0].is_public());
        assert_eq!(history.records[0].file_ids, Some(vec![1853590]));
        assert_eq!(
            history.records[0].files.as_ref().unwrap()[0].file_name,
            "si101.rar"
        );
        assert!(history.records[0].attachments_usable);
        assert_eq!(history.records[1].status, Some(1));
        assert!(!history.records[1].is_public());
        assert!(!history.records[2].attachments_usable);
        assert!(history.records[3].attachments_usable);

        let cached = serde_json::to_value(&profile).unwrap();
        assert!(cached.get("update_history").is_some());
        assert_eq!(
            cached["update_history"]["records"][0]["_aFiles"][0]["_sFile"],
            "si101.rar"
        );
        let restored: ProfileResponse = serde_json::from_value(cached).unwrap();
        assert_eq!(restored.update_history, profile.update_history);
    }

    #[test]
    fn update_history_validator_accepts_complete_pages_and_rejects_partial_or_repeated_pages() {
        let record = |id| UpdateRecord {
            id,
            name: String::new(),
            date_modified: 0,
            date_added: 0,
            profile_url: String::new(),
            html_text: None,
            version: None,
            status: Some(0),
            file_ids: None,
            files: None,
            attachments_usable: true,
            is_private: false,
            is_trashed: false,
            is_withheld: false,
        };
        let metadata = |record_count, is_complete| ApiMetadata {
            record_count,
            per_page: UPDATE_HISTORY_PAGE_SIZE,
            is_complete,
        };

        let complete = ApiEnvelope {
            metadata: metadata(1, true),
            records: vec![record(1)],
        };
        assert_eq!(
            validate_update_history_pages(&[complete])
                .unwrap()
                .records
                .len(),
            1
        );

        let partial = ApiEnvelope {
            metadata: metadata(2, true),
            records: vec![record(1)],
        };
        assert!(validate_update_history_pages(&[partial]).is_err());

        let first_page = ApiEnvelope {
            metadata: metadata(2, false),
            records: vec![record(1), record(2)],
        };
        assert!(validate_update_history_pages(&[first_page]).is_err());

        let repeated_first = ApiEnvelope {
            metadata: metadata(2, false),
            records: vec![record(1)],
        };
        let repeated_last = ApiEnvelope {
            metadata: metadata(2, true),
            records: vec![record(1)],
        };
        assert!(validate_update_history_pages(&[repeated_first, repeated_last]).is_err());
    }

    fn test_mod_file(id: u64, download_url: Option<&str>, is_archived: bool) -> ModFile {
        ModFile {
            id,
            file_name: format!("file-{id}.zip"),
            file_size: 0,
            date_added: 0,
            download_count: 0,
            description: None,
            version: None,
            download_url: download_url.map(str::to_owned),
            is_archived,
            md5: None,
        }
    }

    #[test]
    fn update_history_fetch_eligibility_includes_archived_catalog_entries() {
        let archived_source = test_mod_file(1, None, true);
        let active_candidate = test_mod_file(2, Some("https://example.com/2"), false);
        let profile = ProfileResponse {
            files: vec![active_candidate.clone()],
            archived_files: vec![archived_source],
            ..Default::default()
        };
        assert!(should_fetch_update_history(&profile));

        let source_in_active_array = test_mod_file(3, None, true);
        let profile = ProfileResponse {
            files: vec![source_in_active_array, active_candidate],
            ..Default::default()
        };
        assert!(should_fetch_update_history(&profile));
    }

    #[test]
    fn update_history_fetch_eligibility_requires_distinct_catalog_files_and_active_url() {
        let duplicate_id = test_mod_file(1, Some("https://example.com/1"), false);
        let duplicate_profile = ProfileResponse {
            files: vec![duplicate_id.clone()],
            archived_files: vec![duplicate_id],
            ..Default::default()
        };
        assert!(!should_fetch_update_history(&duplicate_profile));

        let single_file_profile = ProfileResponse {
            files: vec![test_mod_file(1, None, false)],
            ..Default::default()
        };
        assert!(!should_fetch_update_history(&single_file_profile));

        let no_active_download_profile = ProfileResponse {
            archived_files: vec![test_mod_file(1, None, true), test_mod_file(2, None, true)],
            ..Default::default()
        };
        assert!(!should_fetch_update_history(&no_active_download_profile));

        let all_flagged_archived_profile = ProfileResponse {
            files: vec![
                test_mod_file(1, Some("https://example.com/1"), true),
                test_mod_file(2, Some("https://example.com/2"), true),
            ],
            ..Default::default()
        };
        assert!(!should_fetch_update_history(&all_flagged_archived_profile));

        let archived_array_duplicate_profile = ProfileResponse {
            files: vec![test_mod_file(1, Some("https://example.com/1"), false)],
            archived_files: vec![test_mod_file(1, None, true), test_mod_file(2, None, true)],
            ..Default::default()
        };
        assert!(!should_fetch_update_history(
            &archived_array_duplicate_profile
        ));

        let two_active_profile = ProfileResponse {
            files: vec![
                test_mod_file(1, Some("https://example.com/1"), false),
                test_mod_file(2, Some("https://example.com/2"), false),
            ],
            ..Default::default()
        };
        assert!(should_fetch_update_history(&two_active_profile));
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
        || profile.is_trashed
        || profile.is_withheld
}

/// Update history is useful when a profile has more than one catalog entry,
/// including files the author has archived, and at least one currently
/// downloadable file. The active `_aFiles` array supplies candidates, while
/// the archive flag and `_aArchivedFiles` membership still identify archived
/// entries when the API returns inconsistent arrays.
fn should_fetch_update_history(profile: &ProfileResponse) -> bool {
    let mut catalog_file_ids = HashSet::new();
    for file in profile.files.iter().chain(&profile.archived_files) {
        if file.id != 0 {
            catalog_file_ids.insert(file.id);
        }
    }

    catalog_file_ids.len() >= 2
        && profile.files.iter().any(|file| {
            file.id != 0
                && !file.is_archived
                && !profile
                    .archived_files
                    .iter()
                    .any(|archived| archived.id == file.id)
                && file
                    .download_url
                    .as_deref()
                    .is_some_and(|url| !url.trim().is_empty())
        })
}

pub fn install_block_reason(profile: &ProfileResponse) -> Option<String> {
    if profile.is_private {
        Some("This mod is private and cannot be installed automatically.".to_string())
    } else if profile.is_trashed {
        Some(if let Some(trasher) = trashed_by_owner(profile) {
            format!("This mod has been deleted by {}.", trasher.name)
        } else {
            "This mod has been deleted and cannot be installed automatically.".to_string()
        })
    } else if profile.is_withheld {
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
    } else if profile.is_trashed {
        if let Some(trasher) = trashed_by_owner(profile) {
            Some(format!("Mod was deleted by {}", trasher.name))
        } else {
            Some("Mod was deleted".to_string())
        }
    } else if profile.is_withheld {
        if let Some(notice) = withheld_notice(profile) {
            if let Some(withholder) = notice.withholder.as_ref() {
                Some(format!("Mod was withheld by {}", withholder.name))
            } else {
                Some("Mod is now withheld".to_string())
            }
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
    let mut url = Url::parse(&format!("{GAMEBANANA_API_BASE}/Mod/Index"))?;
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
    let mut url = Url::parse(&format!("{GAMEBANANA_API_BASE}/Mod/Index"))?;
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
    let mut url = Url::parse(&format!("{GAMEBANANA_API_BASE}/Mod/Categories"))?;
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
    let mut url = Url::parse(&format!("{GAMEBANANA_API_BASE}/Mod/Index"))?;
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
    let mut url = Url::parse(&format!("{GAMEBANANA_API_BASE}/Util/Search/Results"))?;
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
    let mut url = Url::parse(&format!("{GAMEBANANA_API_BASE}/Util/Search/Results"))?;
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
    let url = format!("{GAMEBANANA_API_BASE}/{kind}/{mod_id}/ProfilePage");
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
    let url = format!("{GAMEBANANA_API_BASE}/{kind}/{mod_id}/ProfilePage");
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
    fetch_updates_page_async(client, mod_id, false, 1).await
}

async fn fetch_updates_page_async(
    client: &ClientWithMiddleware,
    mod_id: u64,
    is_tool: bool,
    page: usize,
) -> Result<ApiEnvelope<UpdateRecord>> {
    let kind = item_api_kind(is_tool);
    let mut url = Url::parse(&format!("{GAMEBANANA_API_BASE}/{kind}/{mod_id}/Updates"))?;
    {
        let mut query_pairs = url.query_pairs_mut();
        query_pairs.append_pair("_nPage", &page.to_string());
        query_pairs.append_pair("_nPerpage", &UPDATE_HISTORY_PAGE_SIZE.to_string());
    }
    let response = client
        .get(url.as_str())
        .send()
        .await
        .with_context(|| format!("failed to fetch GameBanana {kind} updates"))?;
    response
        .error_for_status()
        .with_context(|| format!("GameBanana {kind} updates returned an error"))?
        .json()
        .await
        .with_context(|| format!("failed to parse GameBanana {kind} updates"))
}

const UPDATE_HISTORY_PAGE_SIZE: usize = 50;
const UPDATE_HISTORY_MAX_PAGES: usize = 5;
const UPDATE_HISTORY_ATTACH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Validate and combine the pages returned by the update-log endpoint.
///
/// A history is usable only when every requested page is present, metadata is
/// stable, the record count matches, and every record id is unique. Keeping
/// this as a pure helper makes the neutral-on-failure behavior testable without
/// an HTTP server.
fn validate_update_history_pages(pages: &[ApiEnvelope<UpdateRecord>]) -> Result<UpdateHistory> {
    let Some(first_page) = pages.first() else {
        bail!("GameBanana update history returned no pages");
    };
    let expected_record_count = first_page.metadata.record_count;
    if first_page.metadata.per_page != UPDATE_HISTORY_PAGE_SIZE {
        bail!("GameBanana update history returned an unexpected page size");
    }
    if expected_record_count > UPDATE_HISTORY_MAX_PAGES * UPDATE_HISTORY_PAGE_SIZE {
        bail!("GameBanana update history exceeds the pagination limit");
    }
    if pages.len() > UPDATE_HISTORY_MAX_PAGES {
        bail!("GameBanana update history exceeded the pagination limit");
    }

    let mut records = Vec::new();
    let mut record_ids = HashSet::new();
    for (page_index, page) in pages.iter().enumerate() {
        if page.metadata.record_count != expected_record_count
            || page.metadata.per_page != UPDATE_HISTORY_PAGE_SIZE
        {
            bail!("GameBanana update history metadata changed between pages");
        }
        if page.records.len() > UPDATE_HISTORY_PAGE_SIZE {
            bail!("GameBanana update history returned too many records on a page");
        }

        for record in &page.records {
            if record.id == 0 || !record_ids.insert(record.id) {
                bail!("GameBanana update history contains a duplicate or invalid record id");
            }
        }

        let is_last_page = page_index + 1 == pages.len();
        if page.metadata.is_complete {
            if !is_last_page {
                bail!("GameBanana update history has pages after a complete page");
            }
        } else if is_last_page || page.records.len() != UPDATE_HISTORY_PAGE_SIZE {
            bail!("GameBanana update history contains a partial page");
        }

        records.extend(page.records.iter().cloned());
    }

    if records.len() != expected_record_count {
        bail!("GameBanana update history record count did not match metadata");
    }

    Ok(UpdateHistory { records })
}

/// Fetch a complete update history for either a mod or a tool. Any incomplete
/// or inconsistent history is returned as an error so callers can keep it
/// absent from a cached profile and use the normal matching path.
pub async fn fetch_update_history_async(
    client: &ClientWithMiddleware,
    mod_id: u64,
    is_tool: bool,
) -> Result<UpdateHistory> {
    let mut pages = Vec::new();
    for page_number in 1..=UPDATE_HISTORY_MAX_PAGES {
        let page = fetch_updates_page_async(client, mod_id, is_tool, page_number).await?;
        let is_complete = page.metadata.is_complete;
        pages.push(page);
        if is_complete {
            return validate_update_history_pages(&pages);
        }
    }

    validate_update_history_pages(&pages)
}

/// Best-effort attachment used by profile workers. The cache field is reset
/// before fetching so a failed refresh cannot leave stale or partial history
/// looking complete.
pub async fn attach_update_history_async(
    client: &ClientWithMiddleware,
    profile: &mut ProfileResponse,
    is_tool: bool,
) {
    profile.update_history = None;
    if is_unavailable(profile) {
        return;
    }
    if !should_fetch_update_history(profile) {
        return;
    }

    let history = tokio::time::timeout(
        UPDATE_HISTORY_ATTACH_TIMEOUT,
        fetch_update_history_async(client, profile.id, is_tool),
    )
    .await;
    if let Ok(Ok(history)) = history {
        profile.update_history = Some(history);
    }
}

/// GameBanana's answer for one file's archive contents.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum RawFileList {
    Paths(Vec<String>),
    /// The file is gone or has no readable list; asking again will not help.
    Unavailable,
}

const RAW_FILE_LIST_ATTEMPTS: usize = 3;
const RAW_FILE_LIST_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(2);

/// Fetch the path list inside one file's archive.  GameBanana sometimes
/// answers 200 with an empty body; that is retried, then reported as an
/// error so it is never cached as a real answer.
pub async fn fetch_raw_file_list_async(
    client: &ClientWithMiddleware,
    file_id: u64,
) -> Result<RawFileList> {
    let url = Url::parse(&format!("{GAMEBANANA_API_BASE}/File/{file_id}/RawFileList"))?;
    for attempt in 1..=RAW_FILE_LIST_ATTEMPTS {
        let response = client
            .get(url.as_str())
            .send()
            .await
            .context("failed to fetch GameBanana file list")?;
        if matches!(response.status().as_u16(), 400 | 403 | 404 | 410) {
            return Ok(RawFileList::Unavailable);
        }
        let body = response
            .error_for_status()
            .context("GameBanana file list returned an error")?
            .text()
            .await
            .context("failed to read GameBanana file list")?;
        let paths = crate::integrations::gamebanana_matcher::paths::split_raw_file_list(&body);
        if !paths.is_empty() {
            return Ok(RawFileList::Paths(paths));
        }
        if attempt < RAW_FILE_LIST_ATTEMPTS {
            tokio::time::sleep(RAW_FILE_LIST_RETRY_DELAY).await;
        }
    }
    bail!("GameBanana file list for {file_id} came back empty")
}

/// Per GameBanana's dev guidance, `_aPreviewContent` is the current preview
/// property and `_aPreviewMedia` is the legacy one; a card thumbnail must
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

    let mut add_author = |name: &str| {
        let name = name.trim();
        if !name.is_empty()
            && !authors
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(name))
        {
            authors.push(name.to_owned());
        }
    };

    if let Some(submitter) = &profile.submitter {
        add_author(&submitter.name);
    }
    for credit in &profile.credits {
        if let Some(user) = &credit.user {
            add_author(&user.name);
        } else if let Some(name) = &credit.name {
            add_author(name);
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
