//! Event schedules from verified JP Master data; source timestamps use Japan time.
use crate::error::AppError;
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub id: i64,
    pub name_text_id: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub display_ends_at: DateTime<Utc>,
    pub ranking_enabled: bool,
    pub music_ranking_enabled: bool,
    pub total_music_ranking_enabled: bool,
}

impl Event {
    pub fn from_master(value: &Value) -> Result<Self, AppError> {
        let id = value["_id"]
            .as_i64()
            .filter(|id| *id > 0)
            .ok_or(AppError::MasterDataUnavailable)?;
        let timestamp = |field: &str| -> Result<DateTime<Utc>, AppError> {
            let raw = value[field]
                .as_str()
                .ok_or(AppError::MasterDataUnavailable)?;
            let local = NaiveDateTime::parse_from_str(raw, "%Y/%m/%d %H:%M:%S")
                .map_err(|_| AppError::MasterDataUnavailable)?;
            FixedOffset::east_opt(9 * 3600)
                .unwrap()
                .from_local_datetime(&local)
                .single()
                .map(|time| time.with_timezone(&Utc))
                .ok_or(AppError::MasterDataUnavailable)
        };
        let enabled = |field: &str| {
            value[field]
                .as_bool()
                .map(|disabled| !disabled)
                .ok_or(AppError::MasterDataUnavailable)
        };
        let event = Self {
            id,
            name_text_id: value["_nameTextId"].as_str().map(str::to_owned),
            starts_at: timestamp("_startAt")?,
            ends_at: timestamp("_endAt")?,
            display_ends_at: timestamp("_displayEndAt")?,
            ranking_enabled: enabled("_isRankingDisabled")?,
            music_ranking_enabled: enabled("_isMusicRankingDisabled")?,
            total_music_ranking_enabled: enabled("_isTotalMusicRankingDisabled")?,
        };
        if event.starts_at > event.ends_at || event.ends_at > event.display_ends_at {
            return Err(AppError::MasterDataUnavailable);
        }
        Ok(event)
    }

    pub fn is_active(&self, now: DateTime<Utc>) -> bool {
        self.starts_at <= now && now <= self.ends_at
    }
}

pub fn current(entries: &[Value], now: DateTime<Utc>) -> Result<Event, AppError> {
    let events = entries
        .iter()
        .map(Event::from_master)
        .collect::<Result<Vec<_>, _>>()?;
    let mut active = events.into_iter().filter(|event| event.is_active(now));
    let event = active.next().ok_or(AppError::NotFound)?;
    if active.next().is_some() {
        return Err(AppError::MasterDataUnavailable);
    }
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn entry() -> Value {
        json!({"_id":1,"_nameTextId":"Event_Name_0001",
        "_startAt":"2026/09/30 18:00:00","_endAt":"2026/10/08 20:59:59",
        "_displayEndAt":"2026/10/10 20:59:59","_isRankingDisabled":true,
        "_isMusicRankingDisabled":false,"_isTotalMusicRankingDisabled":false})
    }
    #[test]
    fn jp_schedule_and_ranking_capabilities_are_preserved() {
        let event = Event::from_master(&entry()).unwrap();
        assert_eq!(event.starts_at.to_rfc3339(), "2026-09-30T09:00:00+00:00");
        assert!(!event.ranking_enabled);
        assert!(event.music_ranking_enabled);
        assert!(event.is_active(event.starts_at));
        assert!(event.is_active(event.ends_at));
        assert!(!event.is_active(event.ends_at + chrono::Duration::seconds(1)));
    }
    #[test]
    fn current_event_rejects_missing_ambiguous_and_corrupt_schedules() {
        let now = "2026-10-05T00:00:00Z".parse().unwrap();
        assert_eq!(current(&[entry()], now).unwrap().id, 1);
        assert!(matches!(current(&[], now), Err(AppError::NotFound)));
        assert!(matches!(
            current(&[entry(), entry()], now),
            Err(AppError::MasterDataUnavailable)
        ));
        let mut corrupt = entry();
        corrupt["_isRankingDisabled"] = Value::Null;
        assert!(Event::from_master(&corrupt).is_err());
        corrupt = entry();
        corrupt["_endAt"] = "bad".into();
        assert!(Event::from_master(&corrupt).is_err());
        let after = "2026-10-09T00:00:00Z".parse().unwrap();
        assert!(matches!(
            current(&[entry()], after),
            Err(AppError::NotFound)
        ));
    }
}
