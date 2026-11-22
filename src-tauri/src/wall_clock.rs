use jiff::{tz::TimeZone, Timestamp};

pub(crate) fn utc_rfc3339(instant: Timestamp) -> String {
    // Match the stored RFC 3339 strings: numeric UTC offset and 0/3/6/9 digits.
    // Meeting queries order the timestamp text directly.
    let nanos = instant.subsec_nanosecond();
    let format = if nanos == 0 {
        "%Y-%m-%dT%H:%M:%S+00:00"
    } else if nanos % 1_000_000 == 0 {
        "%Y-%m-%dT%H:%M:%S%.3f+00:00"
    } else if nanos % 1_000 == 0 {
        "%Y-%m-%dT%H:%M:%S%.6f+00:00"
    } else {
        "%Y-%m-%dT%H:%M:%S%.9f+00:00"
    };
    instant.strftime(format).to_string()
}

pub(crate) fn local_meeting_title(instant: Timestamp) -> String {
    meeting_title_in(instant, TimeZone::system())
}

fn meeting_title_in(instant: Timestamp, zone: TimeZone) -> String {
    let local = instant.to_zoned_datetime(zone);
    format!("Meeting {}", local.strftime("%Y-%m-%d %H:%M"))
}

pub(crate) fn utc_run_prefix(instant: Timestamp) -> String {
    instant.strftime("%Y%m%d%H%M%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{
        finish_meeting, get_meeting, initialize_database, insert_meeting, NewMeeting,
    };

    #[test]
    fn keeps_existing_precision_and_utc_offset() {
        for value in [
            "2026-11-22T00:00:00+00:00",
            "2026-11-22T00:00:00.100+00:00",
            "2026-11-22T00:00:00.100200+00:00",
            "2026-11-22T00:00:00.100200300+00:00",
            "1969-12-31T23:59:59.123456+00:00",
        ] {
            let instant = value.parse::<Timestamp>().unwrap();
            assert_eq!(utc_rfc3339(instant), value);
        }
        assert_eq!(
            utc_run_prefix("2026-11-22T00:00:00.100Z".parse().unwrap()),
            "20261122000000"
        );
    }

    #[test]
    fn meeting_title_uses_local_date_and_daylight_saving() {
        let zone = TimeZone::get("America/New_York").unwrap();
        for (instant, expected) in [
            ("2026-01-02T04:30:00Z", "Meeting 2026-01-01 23:30"),
            ("2026-07-02T04:30:00Z", "Meeting 2026-07-02 00:30"),
            ("2026-03-08T06:59:00Z", "Meeting 2026-03-08 01:59"),
            ("2026-03-08T07:00:00Z", "Meeting 2026-03-08 03:00"),
        ] {
            assert_eq!(
                meeting_title_in(instant.parse().unwrap(), zone.clone()),
                expected
            );
        }
    }

    #[test]
    fn new_meeting_timestamps_round_trip_through_storage() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("synthetic.sqlite3");
        initialize_database(&database).unwrap();
        let start = "2026-11-22T00:00:00.100+00:00";
        let end = "2026-11-22T00:01:00.100200+00:00";
        insert_meeting(
            &database,
            &NewMeeting {
                id: "test",
                title: "Synthetic meeting",
                title_source: "datetime_placeholder",
                started_at: &utc_rfc3339(start.parse().unwrap()),
                status: "recording",
                language_hint: "auto",
                summary_language: "auto",
            },
        )
        .unwrap();
        finish_meeting(
            &database,
            "test",
            &utc_rfc3339(end.parse().unwrap()),
            "recorded",
        )
        .unwrap();
        let meeting = get_meeting(&database, "test").unwrap().unwrap();
        assert_eq!(meeting.started_at, start);
        assert_eq!(meeting.ended_at.as_deref(), Some(end));
    }
}
