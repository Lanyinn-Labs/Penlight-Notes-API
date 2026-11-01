//! Gekisou Arena schedules, leagues, and rank tiers from verified JP Master data.
use crate::error::AppError;
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArenaSeason {
    pub id: i64,
    pub name_text_id: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub ranking_count_ends_at: DateTime<Utc>,
}

impl ArenaSeason {
    pub fn from_master(value: &Value) -> Result<Self, AppError> {
        let id = value["_id"]
            .as_i64()
            .filter(|id| *id > 0)
            .ok_or(AppError::MasterDataUnavailable)?;
        let parse_time = |field: &str| -> Result<DateTime<Utc>, AppError> {
            let raw = value[field]
                .as_str()
                .ok_or(AppError::MasterDataUnavailable)?;
            let local = NaiveDateTime::parse_from_str(raw, "%Y/%m/%d %H:%M:%S")
                .or_else(|_| NaiveDateTime::parse_from_str(raw, "%Y/%m/%d %k:%M:%S"))
                .map_err(|_| AppError::MasterDataUnavailable)?;
            FixedOffset::east_opt(9 * 3600)
                .unwrap()
                .from_local_datetime(&local)
                .single()
                .map(|time| time.with_timezone(&Utc))
                .ok_or(AppError::MasterDataUnavailable)
        };
        let season = Self {
            id,
            name_text_id: value["_nameTextId"].as_str().map(str::to_owned),
            starts_at: parse_time("_startAt")?,
            ends_at: parse_time("_endAt")?,
            ranking_count_ends_at: parse_time("_rankingCountEndAt")?,
        };
        if season.starts_at > season.ends_at || season.ends_at > season.ranking_count_ends_at {
            return Err(AppError::MasterDataUnavailable);
        }
        Ok(season)
    }

    pub fn is_active(&self, now: DateTime<Utc>) -> bool {
        self.starts_at <= now && now <= self.ends_at
    }
}

pub fn current_season(entries: &[Value], now: DateTime<Utc>) -> Result<ArenaSeason, AppError> {
    let seasons = entries
        .iter()
        .map(ArenaSeason::from_master)
        .collect::<Result<Vec<_>, _>>()?;
    let mut active = seasons.into_iter().filter(|season| season.is_active(now));
    let season = active.next().ok_or(AppError::NotFound)?;
    if active.next().is_some() {
        return Err(AppError::MasterDataUnavailable);
    }
    Ok(season)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArenaLeague {
    pub id: i64,
    pub name_text_id: Option<String>,
    pub band_id: i64,
    pub voice_character_group: Vec<i64>,
}

impl ArenaLeague {
    pub fn from_master(value: &Value) -> Result<Self, AppError> {
        let id = value["_id"]
            .as_i64()
            .filter(|id| *id > 0)
            .ok_or(AppError::MasterDataUnavailable)?;
        let band_id = value["_bandId"]
            .as_i64()
            .ok_or(AppError::MasterDataUnavailable)?;
        let voice_character_group = value["_voiceCharacterGroup"]
            .as_array()
            .ok_or(AppError::MasterDataUnavailable)?
            .iter()
            .map(|id| id.as_i64().ok_or(AppError::MasterDataUnavailable))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            id,
            name_text_id: value["_nameTextId"].as_str().map(str::to_owned),
            band_id,
            voice_character_group,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArenaRank {
    pub id: i64,
    pub band_id: i64,
    pub name_text_id: Option<String>,
    pub rank: i64,
    pub division: i64,
    pub lp: i64,
    pub season_reset_lp: i64,
    pub point_behavior_type: i64,
    pub is_top_rank: bool,
}

impl ArenaRank {
    pub fn from_master(value: &Value) -> Result<Self, AppError> {
        let id = value["_id"]
            .as_i64()
            .filter(|id| *id > 0)
            .ok_or(AppError::MasterDataUnavailable)?;
        let band_id = value["_bandId"]
            .as_i64()
            .ok_or(AppError::MasterDataUnavailable)?;
        let rank = value["_rank"]
            .as_i64()
            .ok_or(AppError::MasterDataUnavailable)?;
        let division = value["_division"]
            .as_i64()
            .ok_or(AppError::MasterDataUnavailable)?;
        let lp = value["_lp"]
            .as_i64()
            .ok_or(AppError::MasterDataUnavailable)?;
        let season_reset_lp = value["_seasonResetLp"]
            .as_i64()
            .ok_or(AppError::MasterDataUnavailable)?;
        let point_behavior_type = value["_pointBehaviorType"]
            .as_i64()
            .ok_or(AppError::MasterDataUnavailable)?;
        let is_top_rank = value["_isTopRank"]
            .as_bool()
            .ok_or(AppError::MasterDataUnavailable)?;
        Ok(Self {
            id,
            band_id,
            name_text_id: value["_nameTextId"].as_str().map(str::to_owned),
            rank,
            division,
            lp,
            season_reset_lp,
            point_behavior_type,
            is_top_rank,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArenaRankingReward {
    pub id: i64,
    pub league_id: i64,
    pub rank_start: i64,
    pub rank_end: i64,
    pub resource_type: i64,
    pub resource_id: i64,
    pub resource_count: i64,
}

impl ArenaRankingReward {
    pub fn from_master(value: &Value) -> Result<Self, AppError> {
        let id = value["_id"]
            .as_i64()
            .filter(|id| *id > 0)
            .ok_or(AppError::MasterDataUnavailable)?;
        let league_id = value["_leagueId"]
            .as_i64()
            .ok_or(AppError::MasterDataUnavailable)?;
        let rank_start = value["_rankStart"]
            .as_i64()
            .filter(|r| *r > 0)
            .ok_or(AppError::MasterDataUnavailable)?;
        let rank_end = value["_rankEnd"]
            .as_i64()
            .filter(|r| *r >= rank_start)
            .ok_or(AppError::MasterDataUnavailable)?;
        let resource_type = value["_resourceType"]
            .as_i64()
            .ok_or(AppError::MasterDataUnavailable)?;
        let resource_id = value["_resourceId"]
            .as_i64()
            .ok_or(AppError::MasterDataUnavailable)?;
        let resource_count = value["_resourceCount"]
            .as_i64()
            .filter(|c| *c > 0)
            .ok_or(AppError::MasterDataUnavailable)?;
        Ok(Self {
            id,
            league_id,
            rank_start,
            rank_end,
            resource_type,
            resource_id,
            resource_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn season_entry() -> Value {
        json!({
            "_id": 1,
            "_nameTextId": "Arena_Season_Name_0001",
            "_startAt": "2026/10/28 0:00:00",
            "_endAt": "2026/11/28 14:59:58",
            "_rankingCountEndAt": "2026/11/28 14:59:59"
        })
    }

    #[test]
    fn arena_season_schedule_is_parsed_and_active_window_verified() {
        let season = ArenaSeason::from_master(&season_entry()).unwrap();
        assert_eq!(season.id, 1);
        assert_eq!(
            season.name_text_id.as_deref(),
            Some("Arena_Season_Name_0001")
        );
        assert_eq!(season.starts_at.to_rfc3339(), "2026-10-27T15:00:00+00:00");
        assert_eq!(season.ends_at.to_rfc3339(), "2026-11-28T05:59:58+00:00");
        assert!(season.is_active(season.starts_at));
        assert!(season.is_active(season.ends_at));
        assert!(!season.is_active(season.starts_at - chrono::Duration::seconds(1)));
        assert!(!season.is_active(season.ends_at + chrono::Duration::seconds(1)));
    }

    #[test]
    fn current_season_finds_single_active_and_rejects_ambiguity() {
        let now: DateTime<Utc> = "2026-11-01T00:00:00Z".parse().unwrap();
        let entries = vec![
            season_entry(),
            json!({
                "_id": 2,
                "_nameTextId": "Arena_Season_Name_0002",
                "_startAt": "2026/11/28 15:00:00",
                "_endAt": "2026/12/28 14:59:58",
                "_rankingCountEndAt": "2026/12/28 14:59:59"
            }),
        ];
        let current = current_season(&entries, now).unwrap();
        assert_eq!(current.id, 1);

        // Ambiguous: two overlapping active seasons
        let overlapping = vec![
            season_entry(),
            json!({
                "_id": 2,
                "_nameTextId": "Arena_Season_Name_0002",
                "_startAt": "2026/10/28 0:00:00",
                "_endAt": "2026/11/28 14:59:58",
                "_rankingCountEndAt": "2026/11/28 14:59:59"
            }),
        ];
        assert!(matches!(
            current_season(&overlapping, now),
            Err(AppError::MasterDataUnavailable)
        ));

        // Not found: none active
        let past: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        assert!(matches!(
            current_season(&entries, past),
            Err(AppError::NotFound)
        ));
    }

    #[test]
    fn arena_models_parse_valid_master_records() {
        let league_val = json!({
            "_id": 1,
            "_nameTextId": "Arena_League_Name_001",
            "_bandId": 1,
            "_voiceCharacterGroup": [1, 2, 3, 4, 5],
            "_startAt": ""
        });
        let league = ArenaLeague::from_master(&league_val).unwrap();
        assert_eq!(league.id, 1);
        assert_eq!(league.band_id, 1);
        assert_eq!(league.voice_character_group, vec![1, 2, 3, 4, 5]);

        let rank_val = json!({
            "_id": 1,
            "_bandId": 1,
            "_nameTextId": "Arena_Rank_Name_001",
            "_rank": 1,
            "_division": 1,
            "_lp": 0,
            "_seasonResetLp": 0,
            "_startAt": "",
            "_pointBehaviorType": 1,
            "_isTopRank": false
        });
        let rank = ArenaRank::from_master(&rank_val).unwrap();
        assert_eq!(rank.id, 1);
        assert_eq!(rank.division, 1);
        assert!(!rank.is_top_rank);

        let reward_val = json!({
            "_id": 1,
            "_leagueId": 0,
            "_rankStart": 1,
            "_rankEnd": 1,
            "_resourceType": 1,
            "_resourceId": 1,
            "_resourceCount": 10
        });
        let reward = ArenaRankingReward::from_master(&reward_val).unwrap();
        assert_eq!(reward.id, 1);
        assert_eq!(reward.rank_start, 1);
        assert_eq!(reward.rank_end, 1);
    }
}
