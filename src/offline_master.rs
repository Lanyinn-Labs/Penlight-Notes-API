//! Master table schemas and optional decrypted records recovered from each APK.

use std::{borrow::Cow, collections::HashMap, path::Path, sync::OnceLock};

use serde_json::{json, Value};

use crate::{error::AppError, region::Region};

static GLOBAL_SCHEMA: OnceLock<Value> = OnceLock::new();
static JP_SCHEMA: OnceLock<Value> = OnceLock::new();

fn schema(region: Region) -> &'static Value {
    match region {
        Region::Global => GLOBAL_SCHEMA.get_or_init(|| {
            serde_json::from_str(include_str!("../data/master-schema-global.json"))
                .expect("bundled global Master schema must be valid JSON")
        }),
        Region::Jp => JP_SCHEMA.get_or_init(|| {
            serde_json::from_str(include_str!("../data/master-schema-jp.json"))
                .expect("bundled JP Master schema must be valid JSON")
        }),
    }
}

pub fn list(region: Region) -> Value {
    let schema = schema(region);
    let entries: Vec<Value> = schema["tables"]
        .as_array()
        .expect("bundled Master tables must be an array")
        .iter()
        .map(|table| {
            json!({
                "name": table["name"],
                "model_type": table["model_type"],
                "model_found": table["model_found"],
                "record_count": table["record_count"],
                "field_count": table["fields"].as_array().map_or(0, Vec::len),
            })
        })
        .collect();
    json!({
        "region": schema["region"],
        "source": schema["source"],
        "client_version": schema["client_version"],
        "apk_sha256": schema["apk_sha256"],
        "records_available": false,
        "entries": entries,
    })
}

pub fn get(region: Region, name: &str) -> Result<Value, AppError> {
    let schema = schema(region);
    let table = schema["tables"]
        .as_array()
        .expect("bundled Master tables must be an array")
        .iter()
        .find(|table| table["name"].as_str() == Some(name))
        .ok_or(AppError::NotFound)?;
    Ok(json!({
        "region": schema["region"],
        "source": schema["source"],
        "client_version": schema["client_version"],
        "apk_sha256": schema["apk_sha256"],
        "records_available": false,
        "table": table,
    }))
}

pub async fn records(region: Region, directory: &Path, name: &str) -> Result<Value, AppError> {
    let schema = schema(region);
    let known = schema["tables"]
        .as_array()
        .expect("bundled Master tables must be an array")
        .iter()
        .any(|table| table["name"].as_str() == Some(name));
    if !known {
        return Err(AppError::NotFound);
    }

    let summary = tokio::fs::read(directory.join("summary.json"))
        .await
        .map_err(|_| AppError::MasterDataUnavailable)?;
    let summary: Value =
        serde_json::from_slice(&summary).map_err(|_| AppError::MasterDataUnavailable)?;
    let expected_hash = match region {
        Region::Global => &summary["source_apk_sha256"],
        Region::Jp => &summary["source_asset_pack_apk_sha256"],
    };
    if expected_hash != &schema["apk_sha256"]
        || (region == Region::Jp && summary["source_base_apk_sha256"] != schema["base_apk_sha256"])
    {
        return Err(AppError::MasterDataUnavailable);
    }

    let content = tokio::fs::read(directory.join(format!("{name}.json")))
        .await
        .map_err(|_| AppError::MasterDataUnavailable)?;
    let entries = tokio::task::spawn_blocking(move || {
        let mut document: Value =
            serde_json::from_slice(&content).map_err(|_| AppError::MasterDataUnavailable)?;
        document
            .as_object_mut()
            .and_then(|object| object.remove("_allData"))
            .filter(Value::is_array)
            .ok_or(AppError::MasterDataUnavailable)
    })
    .await
    .map_err(|_| AppError::MasterDataUnavailable)??;
    Ok(json!({
        "region": schema["region"],
        "source": "apk_master_snapshot",
        "client_version": schema["client_version"],
        "apk_sha256": schema["apk_sha256"],
        "entries": entries,
    }))
}

pub const JP_RESOURCES: &[&str] = &[
    "cards",
    "music",
    "events",
    "characters",
    "bands",
    "gacha",
    "items",
    "stamps",
    "shops",
    "login-bonuses",
];

pub(crate) struct CatalogSpec {
    pub(crate) table: &'static str,
    name_field: Option<&'static str>,
    subtitle_field: Option<&'static str>,
}

impl CatalogSpec {
    pub(crate) fn needs_text(&self, document: &Value) -> bool {
        self.name_field.is_some()
            && document["entries"]
                .as_array()
                .is_some_and(|entries| !entries.is_empty())
    }
}

pub(crate) fn catalog_spec(resource: &str) -> Option<CatalogSpec> {
    let (table, name_field, subtitle_field) = match resource {
        "cards" => (
            "MasterMemberCard",
            Some("_nameTextID"),
            Some("_subtitleTextID"),
        ),
        "music" => ("MasterLiveMusic", Some("_titleTextID"), None),
        "events" => ("MasterEvent", None, None),
        "characters" => ("MasterCharacter", Some("_nameTextID"), None),
        "bands" => ("MasterBand", Some("_nameTextID"), None),
        "gacha" => ("MasterGacha", Some("_nameTextId"), None),
        "items" => ("MasterItem", Some("_nameTextId"), None),
        "stamps" => ("MasterStamp", Some("_nameTextId"), None),
        "shops" => ("MasterShop", Some("_nameTextId"), None),
        "login-bonuses" => ("MasterLoginBonus", Some("_nameTextID"), None),
        _ => return None,
    };
    Some(CatalogSpec {
        table,
        name_field,
        subtitle_field,
    })
}

pub async fn jp_catalog(directory: &Path, resource: &str) -> Result<Value, AppError> {
    let spec = catalog_spec(resource).ok_or(AppError::NotFound)?;
    let document = records(Region::Jp, directory, spec.table).await?;
    let text_document = if spec.needs_text(&document) {
        Some(records(Region::Jp, directory, "MasterText").await?)
    } else {
        None
    };
    let resource = resource.to_owned();
    tokio::task::spawn_blocking(move || {
        normalize_jp_catalog(document, text_document.as_ref(), &resource)
    })
    .await
    .map_err(|_| AppError::MasterDataUnavailable)?
}

pub fn normalize_jp_catalog(
    mut document: Value,
    text_document: Option<&Value>,
    resource: &str,
) -> Result<Value, AppError> {
    let spec = catalog_spec(resource).ok_or(AppError::NotFound)?;
    let needs_text = spec.needs_text(&document);
    let entries = document["entries"]
        .as_array_mut()
        .ok_or(AppError::MasterDataUnavailable)?;

    if needs_text {
        let text_document = text_document.ok_or(AppError::MasterDataUnavailable)?;
        let texts: HashMap<Cow<'_, str>, &str> = text_document["entries"]
            .as_array()
            .ok_or(AppError::MasterDataUnavailable)?
            .iter()
            .filter_map(|entry| Some((text_key(&entry["_id"])?, entry["_japanese"].as_str()?)))
            .collect();
        enrich_catalog_entries(entries, &spec, Some(&texts))?;
    } else {
        enrich_catalog_entries(entries, &spec, None)?;
    }
    document["resource"] = json!(resource);
    document["table"] = json!(spec.table);
    Ok(document)
}

fn enrich_catalog_entries(
    entries: &mut [Value],
    spec: &CatalogSpec,
    texts: Option<&HashMap<Cow<'_, str>, &str>>,
) -> Result<(), AppError> {
    for entry in entries {
        let object = entry
            .as_object_mut()
            .ok_or(AppError::MasterDataUnavailable)?;
        if let Some(id) = object.get("_id").cloned() {
            object.insert("id".into(), id);
        }
        if let Some(texts) = texts {
            for (field, output) in [
                (spec.name_field, "name_ja"),
                (spec.subtitle_field, "subtitle_ja"),
            ] {
                if let Some(text) = field
                    .and_then(|field| object.get(field))
                    .and_then(text_key)
                    .and_then(|id| texts.get(&id))
                {
                    object.insert(output.into(), Value::String((*text).to_owned()));
                }
            }
        }
    }
    Ok(())
}

fn text_key(value: &Value) -> Option<Cow<'_, str>> {
    match value {
        Value::String(key) => Some(Cow::Borrowed(key)),
        Value::Number(key) => Some(Cow::Owned(key.to_string())),
        _ => None,
    }
}

pub async fn jp_catalog_entry(
    directory: &Path,
    resource: &str,
    id: &str,
) -> Result<Value, AppError> {
    let id: u64 = id.parse().map_err(|_| AppError::InvalidMasterId)?;
    if id == 0 {
        return Err(AppError::InvalidMasterId);
    }
    let mut document = jp_catalog(directory, resource).await?;
    let entry = document["entries"]
        .as_array_mut()
        .ok_or(AppError::MasterDataUnavailable)?
        .iter()
        .find(|entry| entry["_id"].as_u64() == Some(id))
        .cloned()
        .ok_or(AppError::NotFound)?;
    document
        .as_object_mut()
        .expect("catalog response is an object")
        .remove("entries");
    document["entry"] = entry;
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_text_ids_preserve_string_and_numeric_matches() {
        let document = json!({"entries":[
            {"_id":1,"_nameTextID":"name","_subtitleTextID":42},
            {"_id":2,"_nameTextID":"missing"}
        ]});
        let texts = json!({"entries":[
            {"_id":"name","_japanese":"名前"},
            {"_id":42,"_japanese":"副題"}
        ]});
        let result = normalize_jp_catalog(document, Some(&texts), "cards").unwrap();
        assert_eq!(result["entries"][0]["id"], 1);
        assert_eq!(result["entries"][0]["name_ja"], "名前");
        assert_eq!(result["entries"][0]["subtitle_ja"], "副題");
        assert!(result["entries"][1].get("name_ja").is_none());
    }
}
