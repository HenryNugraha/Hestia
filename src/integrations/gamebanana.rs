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

    (user.is_some() || name.is_some() || role.is_some()).then_some(CreditEntry {
        user,
        name,
        role,
    })
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
    #[serde(rename = "_aCredits", default, deserialize_with = "deserialize_credits")]
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
            profile.credits[1]
                .user
                .as_ref()
                .unwrap()
                .profile_url,
            "https://gamebanana.com/members/2"
        );
        assert_eq!(profile.credits[1].role.as_deref(), Some("Developer"));
        assert_eq!(
            all_authors(&profile),
            vec!["Submitter".to_owned(), "YouWhenMe".to_owned(), "Dimit".to_owned()]
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
        assert_eq!(singleton_group.credits[0].user.as_ref().unwrap().name, "Dimit");
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
        assert_eq!(unavailable_reason(&profile).as_deref(), Some("Mod was deleted"));
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
        assert_eq!(unavailable_reason(&profile).as_deref(), Some("Mod is now withheld"));
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
    let mut url = Url::parse(&format!("{GAMEBANANA_API_BASE}/Mod/{mod_id}/Updates"))?;
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
