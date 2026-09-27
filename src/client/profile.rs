use serde_json::{json, Value};

fn integer(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_str()?.parse().ok())
}
fn text<'a>(texts: &'a Value, id: &Value) -> Option<&'a str> {
    let id = id.as_str()?;
    texts["entries"]
        .as_array()?
        .iter()
        .find(|row| row["_id"].as_str() == Some(id))?["_japanese"]
        .as_str()
        .filter(|value| !value.is_empty())
}

pub(super) fn summarize(
    raw: &Value,
    ranks: Option<&Value>,
    cards: Option<&Value>,
    texts: Option<&Value>,
    version: Option<&str>,
) -> Value {
    let profile = &raw["playerProfile"];
    let exp = integer(&profile["rankExp"]).filter(|exp| *exp >= 0);
    let level = ranks
        .and_then(|table| table["entries"].as_array())
        .and_then(|rows| {
            rows.iter()
                .filter_map(|row| Some((integer(&row["_exp"])?, integer(&row["_rank"])?)))
                .filter(|(threshold, rank)| {
                    *threshold >= 0 && *rank > 0 && exp.is_some_and(|exp| exp >= *threshold)
                })
                .max_by_key(|(threshold, _)| *threshold)
                .map(|(_, rank)| rank)
        });
    let master_id = integer(&profile["favoriteMemberCardMasterId"]).filter(|id| *id > 0);
    let card = cards
        .and_then(|table| table["entries"].as_array())
        .and_then(|rows| {
            rows.iter()
                .find(|row| integer(&row["_id"]) == master_id && master_id.is_some())
        });
    let name = card.and_then(|card| texts.and_then(|texts| text(texts, &card["_nameTextID"])));
    let subtitle =
        card.and_then(|card| texts.and_then(|texts| text(texts, &card["_subtitleTextID"])));
    let updated = integer(&profile["lastUpdatedAt"])
        .filter(|stamp| *stamp > 0)
        .and_then(|stamp| chrono::DateTime::from_timestamp(stamp, 0));
    json!({"player_id":profile["id"], "profile_id":profile["profileId"], "name":profile["name"],
        "player_level":level, "rank_exp":exp,
        "last_updated_at":updated.map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        "favorite_member_card":{"master_id":master_id,"name":name,"subtitle":subtitle},
        "master_version":version, "master_status":if ranks.is_some() && cards.is_some() && texts.is_some() {"ready"} else if ranks.is_some() || cards.is_some() || texts.is_some() {"partial"} else {"unavailable"}})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_summary_resolves_thresholds_and_handles_unavailable_data() {
        let raw = json!({"playerProfile":{"rankExp":532500,"lastUpdatedAt":"1790473348","favoriteMemberCardMasterId":"48"}});
        let ranks = json!({"entries":[{"_rank":25,"_exp":600000},{"_rank":24,"_exp":509000},{"_rank":1,"_exp":0}]});
        let cards =
            json!({"entries":[{"_id":48,"_nameTextID":"name","_subtitleTextID":"subtitle"}]});
        let texts = json!({"entries":[{"_id":"name","_japanese":"矢倉 蓬咲"},{"_id":"subtitle","_japanese":"ほんの少しの勇気を鳴らして"}]});
        let summary = summarize(&raw, Some(&ranks), Some(&cards), Some(&texts), Some("v"));
        assert_eq!(summary["player_level"], 24);
        assert_eq!(summary["last_updated_at"], "2026-09-27T01:42:28Z");
        assert_eq!(summary["favorite_member_card"]["name"], "矢倉 蓬咲");
        let absent = summarize(&raw, None, None, None, None);
        assert!(absent["player_level"].is_null());
        assert_eq!(absent["master_status"], "unavailable");
        assert_eq!(absent["last_updated_at"], summary["last_updated_at"]);
        let invalid = summarize(
            &json!({"playerProfile":{"rankExp":-1,"lastUpdatedAt":"bad"}}),
            Some(&ranks),
            Some(&cards),
            Some(&texts),
            Some("v"),
        );
        assert!(invalid["player_level"].is_null());
        assert!(invalid["last_updated_at"].is_null());
        assert!(invalid["favorite_member_card"]["name"].is_null());
    }
}
