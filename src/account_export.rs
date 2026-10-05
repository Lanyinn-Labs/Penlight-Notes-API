//! Versioned account snapshots for offline tools; only calculation inputs are exported.
use crate::error::AppError;
use serde_json::{json, Value};
use std::collections::HashSet;

fn number(row: &Value, key: &str, minimum: i64) -> Result<i64, AppError> {
    let value = match row.get(key) {
        None => 0,
        Some(value) => value
            .as_i64()
            .or_else(|| value.as_str()?.parse().ok())
            .ok_or(AppError::UpstreamInvalidResponse)?,
    };
    if value < minimum || value > i64::from(i32::MAX) {
        return Err(AppError::UpstreamInvalidResponse);
    }
    Ok(value)
}

fn rows<'a>(data: &'a Value, key: &str) -> Result<Vec<&'a Value>, AppError> {
    match data.get(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(rows)) if rows.iter().all(Value::is_object) => Ok(rows.iter().collect()),
        _ => Err(AppError::UpstreamInvalidResponse),
    }
}

fn collection(
    data: &Value,
    key: &str,
    identity: &str,
    fields: &[(&str, &str, i64)],
) -> Result<Vec<Value>, AppError> {
    let mut seen = HashSet::new();
    rows(data, key)?
        .into_iter()
        .map(|row| {
            let id = number(row, identity, 1)?;
            if !seen.insert(id) {
                return Err(AppError::UpstreamInvalidResponse);
            }
            let mut output = serde_json::Map::new();
            for &(source, target, minimum) in fields {
                output.insert(target.into(), json!(number(row, source, minimum)?));
            }
            Ok(Value::Object(output))
        })
        .collect()
}

pub fn snapshot(raw: &Value, exported_at: &str) -> Result<Value, AppError> {
    let data = raw
        .get("playerData")
        .filter(|value| value.is_object())
        .ok_or(AppError::UpstreamInvalidResponse)?;
    let profile = &data["myProfile"];
    let profile_id = profile["profileId"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| profile["profileId"].as_u64().map(|id| id.to_string()))
        .filter(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()))
        .ok_or(AppError::UpstreamInvalidResponse)?;
    let name = profile["name"]
        .as_str()
        .ok_or(AppError::UpstreamInvalidResponse)?;
    let members = collection(
        data,
        "memberCards",
        "masterId",
        &[
            ("masterId", "master_id", 1),
            ("exp", "exp", 0),
            ("cardRank", "rank", 1),
            ("awakeCount", "awake", 1),
            ("liveSkillLevel", "live_skill_level", 1),
        ],
    )?;
    let snapshots = collection(
        data,
        "supportCards",
        "masterId",
        &[
            ("masterId", "master_id", 1),
            ("exp", "exp", 0),
            ("cardRank", "rank", 1),
        ],
    )?;
    let characters = collection(
        data,
        "characterRank",
        "characterId",
        &[("characterId", "character_id", 1), ("exp", "exp", 0)],
    )?;
    let band_items = collection(
        data,
        "bandItems",
        "masterId",
        &[("masterId", "master_id", 1), ("level", "level", 0)],
    )?;
    let mut decks = Vec::new();
    let mut seen = HashSet::new();
    for deck in rows(data, "decks")? {
        let id = number(deck, "id", 1)?;
        if !seen.insert(id) {
            return Err(AppError::UpstreamInvalidResponse);
        }
        let mut slots = Vec::new();
        let mut indices = HashSet::new();
        for card in rows(deck, "cards")? {
            let index = number(card, "slotIndex", 0)?;
            if !indices.insert(index) {
                return Err(AppError::UpstreamInvalidResponse);
            }
            slots.push(json!({"index":index,
                "member_id":number(card,"memberCardId",-1)?.max(0),
                "snapshot_id":number(card,"supportCardId",-1)?.max(0),
                "trigger_index":number(card,"performanceOrderIndex",0)?}));
        }
        slots.sort_by_key(|slot| slot["index"].as_i64());
        decks.push(json!({"id":id, "name":deck["name"].as_str().unwrap_or(""), "slots":slots}));
    }
    Ok(json!({"schema":"ournotes-account@1", "region":"jp",
        "source":"official_game_service", "exported_at":exported_at,
        "profile":{"profile_id":profile_id,"name":name},
        "members":members,"snapshots":snapshots,"characters":characters,
        "band_items":band_items,"vip_points":number(&data["vip"],"point",0)?,
        "main_deck":number(data,"mainDeck",0)?,"decks":decks}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn account() -> Value {
        json!({"credential":"SECRET","playerData":{
            "myProfile":{"profileId":"12345678901","name":"fixture","id":"PRIVATE_ID"},
            "memberCards":[{"masterId":"51","cardRank":3,"awakeCount":4,"liveSkillLevel":2,"credential":"SECRET"}],
            "decks":[{"id":1,"cards":[{"memberCardId":"51"}]}],
            "gem":{"paid":999},"linkedAccountTypes":[1],"authorizationKey":"SECRET"}})
    }
    #[test]
    fn export_whitelists_calculation_fields_and_preserves_protobuf_zeroes() {
        let snapshot = snapshot(&account(), "2026-10-05T00:00:00Z").unwrap();
        let text = snapshot.to_string();
        for secret in [
            "SECRET",
            "PRIVATE_ID",
            "credential",
            "authorizationKey",
            "gem",
            "linkedAccountTypes",
        ] {
            assert!(!text.contains(secret));
        }
        assert_eq!(snapshot["members"][0]["exp"], 0);
        assert_eq!(snapshot["decks"][0]["slots"][0]["index"], 0);
        assert_eq!(snapshot["snapshots"], json!([]));
        assert_eq!(snapshot["vip_points"], 0);
        assert_eq!(snapshot["profile"]["profile_id"], "12345678901");
    }
    #[test]
    fn empty_game_deck_sentinels_export_as_zero() {
        let mut raw = account();
        raw["playerData"]["decks"][0]["cards"][0]["memberCardId"] = json!("-1");
        raw["playerData"]["decks"][0]["cards"][0]["supportCardId"] = json!("-1");
        let result = snapshot(&raw, "now").unwrap();
        assert_eq!(result["decks"][0]["slots"][0]["member_id"], 0);
        assert_eq!(result["decks"][0]["slots"][0]["snapshot_id"], 0);
        raw["playerData"]["decks"][0]["cards"][0]["memberCardId"] = json!(-2);
        assert!(snapshot(&raw, "now").is_err());
    }
    #[test]
    fn malformed_or_duplicate_owned_cards_fail_without_partial_exports() {
        for value in [
            json!(-1),
            json!(true),
            json!("invalid"),
            json!(2147483648i64),
        ] {
            let mut raw = account();
            raw["playerData"]["memberCards"][0]["exp"] = value;
            assert!(snapshot(&raw, "now").is_err());
        }
        let mut raw = account();
        let duplicate = raw["playerData"]["memberCards"][0].clone();
        raw["playerData"]["memberCards"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        assert!(snapshot(&raw, "now").is_err());
        raw["playerData"]["memberCards"] = json!(null);
        assert!(snapshot(&raw, "now").is_err());
    }
}
