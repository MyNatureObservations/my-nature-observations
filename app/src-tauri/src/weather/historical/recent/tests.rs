use super::*;
use serde_json::{Value, json};
fn now() -> OffsetDateTime {
    OffsetDateTime::parse("2026-10-30T12:00:00Z", &Rfc3339).unwrap()
}
fn request(date: &str) -> PreviewRequest {
    PreviewRequest {
        date: date.into(),
        place_name: "Synthetic lake".into(),
        latitude: 59.3,
        longitude: 18.1,
    }
}
pub(in crate::weather::historical) fn response() -> Value {
    json!({"latitude":59.297,"longitude":18.16,"timezone":"Europe/Stockholm","utc_offset_seconds":7200,
    "current_units":{"time":"iso8601","interval":"seconds","temperature_2m":"°C","relative_humidity_2m":"%","precipitation":"mm","weather_code":"wmo code","wind_speed_10m":"m/s"},
    "current":{"time":"2026-10-30T13:45","interval":900,"temperature_2m":0,"relative_humidity_2m":74,"precipitation":0,"weather_code":0,"wind_speed_10m":0}})
}
fn daily(date: &str) -> Value {
    let mut v = super::super::tests::response();
    v["daily"]["time"] = json!([date]);
    v["daily"]["sunrise"] = json!([format!("{date}T07:00")]);
    v["daily"]["sunset"] = json!([format!("{date}T17:00")]);
    v
}
fn body(v: Value) -> (u16, Vec<u8>) {
    (200, serde_json::to_vec(&v).unwrap())
}
#[test]
fn visit_daily_today_recent_and_history_never_request_current_conditions() {
    for date in ["2026-10-30", "2026-10-29", "2020-06-01"] {
        let (preview, today) = preview_daily(&request(date), now(), |url| {
            let q: BTreeMap<_, _> = url.query_pairs().into_owned().collect();
            assert!(!q.contains_key("current"));
            assert_eq!(q["daily"], DAILY);
            assert_eq!(q["start_date"], date);
            assert_eq!(q["end_date"], date);
            assert_eq!(q["latitude"], "59.3");
            assert_eq!(q["longitude"], "18.1");
            Ok(body(daily(date)))
        })
        .unwrap();
        let f = serde_json::to_value(preview).unwrap()["fetched"].clone();
        assert_eq!(today, date == "2026-10-30");
        assert!(f["validAt"].is_null());
        assert!(f["temperatureC"].is_null());
        assert_eq!(f["period"]["date"], date);
        assert_eq!(
            f["source"]["product"],
            if date == "2020-06-01" {
                "historical-weather"
            } else {
                "forecast-daily"
            }
        );
        assert_eq!(
            f["source"]["dataKind"],
            if date == "2020-06-01" {
                "reanalysis"
            } else {
                "forecast"
            }
        );
    }
}
#[test]
fn visit_daily_uses_service_local_day_at_utc_midnight_boundaries() {
    for (stamp, date, offset, zone) in [
        (
            "2026-10-30T00:15:00Z",
            "2026-10-29",
            -36000,
            "Pacific/Honolulu",
        ),
        (
            "2026-10-30T23:15:00Z",
            "2026-10-31",
            50400,
            "Pacific/Kiritimati",
        ),
    ] {
        let now = OffsetDateTime::parse(stamp, &Rfc3339).unwrap();
        let (p, today) = preview_daily(&request(date), now, |_| {
            let mut response = daily(date);
            response["utc_offset_seconds"] = json!(offset);
            response["timezone"] = json!(zone);
            Ok(body(response))
        })
        .unwrap();
        assert!(today);
        assert_eq!(
            serde_json::to_value(p).unwrap()["fetched"]["period"]["date"],
            date
        );
    }
    assert_eq!(
        preview_daily(&request("2026-11-02"), now(), |_| panic!(
            "unsupported date must not fetch"
        ))
        .err(),
        Some(Error::DateUnavailable)
    );
}

#[test]
fn visit_daily_missing_values_units_and_transport_failures_are_not_current_fallbacks() {
    for (status, expected) in [(503, Error::ServiceUnavailable), (429, Error::RateLimited)] {
        assert_eq!(
            preview_daily(&request("2026-10-30"), now(), |_| Ok((status, Vec::new()))).err(),
            Some(expected)
        );
    }
    assert_eq!(
        preview_daily(&request("2026-10-30"), now(), |_| Err(Error::Network)).err(),
        Some(Error::Network)
    );
    let mut missing = daily("2026-10-30");
    for key in [
        "temperature_2m_mean",
        "temperature_2m_min",
        "temperature_2m_max",
        "precipitation_sum",
        "wind_speed_10m_max",
        "weather_code",
    ] {
        missing["daily"][key] = Value::Null;
    }
    assert_eq!(
        preview_daily(&request("2026-10-30"), now(), |_| Ok(body(missing.clone()))).err(),
        Some(Error::NoData)
    );
    let mut invalid = daily("2026-10-30");
    invalid["daily_units"]["temperature_2m_mean"] = json!("°F");
    assert_eq!(
        preview_daily(&request("2026-10-30"), now(), |_| Ok(body(invalid.clone()))).err(),
        Some(Error::InvalidResponse)
    );
    assert_eq!(
        preview_daily(&request("2026-10-30"), now(), |_| Ok(body(response()))).err(),
        Some(Error::InvalidResponse)
    );
}
#[test]
fn local_calendar_routing_includes_both_boundaries_and_rejects_future() {
    let utc = UtcOffset::UTC;
    for (day, expected) in [
        ("2026-10-30", Route::Current),
        ("2026-10-29", Route::Daily),
        ("2026-10-24", Route::Daily),
        ("2026-10-23", Route::Era5),
    ] {
        let date = Date::parse(
            day,
            &format_description::parse_borrowed::<2>("[year]-[month]-[day]").unwrap(),
        )
        .unwrap();
        assert_eq!(route(date, now(), utc).unwrap(), expected);
    }
    let date = now().date();
    let midnight = OffsetDateTime::parse("2026-10-30T00:15:00Z", &Rfc3339).unwrap();
    assert_eq!(
        route(date, midnight, UtcOffset::from_hms(-10, 0, 0).unwrap()),
        Err(Error::DateUnavailable)
    );
    assert_eq!(
        route(date, midnight, UtcOffset::from_hms(5, 30, 0).unwrap()).unwrap(),
        Route::Current
    );
    assert_eq!(
        route(
            date,
            midnight - time::Duration::hours(1),
            UtcOffset::from_hms(14, 0, 0).unwrap()
        )
        .unwrap(),
        Route::Current
    );
}
#[test]
fn current_fields_never_become_daily_values_and_keep_valid_time_separate() {
    let result = preview(&request("2026-10-30"), now(), |url| {
        assert_eq!(url.host_str(), Some("api.open-meteo.com"));
        let q: BTreeMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q["models"], "ecmwf_ifs");
        assert_eq!(q["forecast_days"], "0");
        assert_eq!(q["current"], CURRENT);
        assert!(!q.contains_key("daily"));
        Ok(body(response()))
    })
    .unwrap();
    let value = serde_json::to_value(result).unwrap();
    let f = &value["fetched"];
    assert_eq!(f["temperatureC"].as_f64(), Some(0.0));
    assert_eq!(f["precipitationMm"].as_f64(), Some(0.0));
    assert_eq!(f["windSpeedMs"].as_f64(), Some(0.0));
    assert!(f["temperatureMeanC"].is_null());
    assert!(f["temperatureMinC"].is_null());
    assert!(f["windSpeedMaxMs"].is_null());
    assert!(f["precipitationSumMm"].is_null());
    assert_eq!(f["validAt"], "2026-10-30T13:45:00+02:00");
    assert_eq!(f["retrievedAt"], "2026-10-30T12:00:00Z");
    assert_eq!(f["currentIntervalSeconds"], 900);
    assert_eq!(f["source"]["dataKind"], "forecast");
    super::super::super::validate_stored_parts("e", None, Some(&f.to_string())).unwrap();
    let mut invalid = f.clone();
    invalid["temperatureMeanC"] = json!(4);
    assert!(
        super::super::super::validate_stored_parts("e", None, Some(&invalid.to_string())).is_err()
    );
    invalid = f.clone();
    invalid.as_object_mut().unwrap().remove("validAt");
    assert!(
        super::super::super::validate_stored_parts("e", None, Some(&invalid.to_string())).is_err()
    );
}
#[test]
fn recent_closed_day_uses_daily_and_old_day_keeps_era5() {
    for date in ["2026-10-29", "2026-10-24", "2026-10-23", "2020-06-01"] {
        let mut calls = Vec::new();
        let result = preview(&request(date), now(), |url| {
            calls.push(url.clone());
            if url.query_pairs().any(|(k, _)| k == "current") {
                Ok(body(response()))
            } else {
                Ok(body(daily(date)))
            }
        })
        .unwrap();
        let last = calls.last().unwrap();
        let era = date <= "2026-10-23";
        assert_eq!(
            last.host_str(),
            Some(if era {
                "archive-api.open-meteo.com"
            } else {
                "api.open-meteo.com"
            })
        );
        assert_eq!(
            result.fetched.source.data_kind,
            if era {
                DataKind::Reanalysis
            } else {
                DataKind::Forecast
            }
        );
        assert!(result.fetched.valid_at.is_none());
        assert_eq!(
            result.fetched.source.product,
            if era {
                "historical-weather"
            } else {
                "forecast-daily"
            }
        );
        assert_eq!(calls.len(), if date == "2020-06-01" { 1 } else { 2 });
    }
}
#[test]
fn missing_zero_malformed_future_and_stale_values_are_distinct() {
    let mut partial = response();
    partial["current"]["temperature_2m"] = Value::Null;
    let result = preview(&request("2026-10-30"), now(), |_| Ok(body(partial.clone()))).unwrap();
    assert!(result.fetched.temperature_c.is_none());
    assert_eq!(result.fetched.precipitation_mm, Some(0.0));
    for (key, value) in [
        ("temperature_2m", json!("invalid")),
        ("precipitation", json!(-1)),
        ("relative_humidity_2m", json!(101)),
        ("time", json!("2026-10-31T00:00")),
        ("time", json!("2026-10-30T02:00")),
        ("interval", json!(0)),
    ] {
        let mut v = response();
        v["current"][key] = value;
        assert!(
            preview(&request("2026-10-30"), now(), |_| Ok(body(v.clone()))).is_err(),
            "{key}"
        );
    }
    let mut empty = response();
    for key in [
        "temperature_2m",
        "precipitation",
        "relative_humidity_2m",
        "wind_speed_10m",
        "weather_code",
    ] {
        empty["current"][key] = Value::Null;
    }
    assert!(matches!(
        preview(&request("2026-10-30"), now(), |_| Ok(body(empty.clone()))),
        Err(Error::NoData)
    ));
    assert!(matches!(
        preview(&request("2026-11-04"), now(), |_| panic!("no HTTP")),
        Err(Error::DateUnavailable)
    ));
    assert!(matches!(
        preview(&request("2026-10-31"), now(), |_| Ok(body(response()))),
        Err(Error::DateUnavailable)
    ));
}
#[test]
fn failed_requests_or_units_cannot_form_a_preview() {
    for err in [Error::Network, Error::Timeout] {
        assert!(
            preview(&request("2026-10-30"), now(), |_| Err(
                if err == Error::Network {
                    Error::Network
                } else {
                    Error::Timeout
                }
            ))
            .is_err()
        );
    }
    for (status, bytes) in [
        (500, b"bad".to_vec()),
        (200, b"{}".to_vec()),
        (400, b"{}".to_vec()),
    ] {
        assert!(
            preview(&request("2026-10-30"), now(), |_| Ok((
                status,
                bytes.clone()
            )))
            .is_err()
        );
    }
    let mut v = response();
    v["current_units"]["wind_speed_10m"] = json!("km/h");
    assert!(preview(&request("2026-10-30"), now(), |_| Ok(body(v.clone()))).is_err());
}

#[test]
fn api_local_day_overrides_utc_day_and_gmt_is_normalized() {
    for (now_text, day, offset, valid) in [
        (
            "2026-10-30T00:15:00Z",
            "2026-10-29",
            -36000,
            "2026-10-29T14:00",
        ),
        (
            "2026-10-29T23:15:00Z",
            "2026-10-30",
            50400,
            "2026-10-30T13:00",
        ),
        ("2026-10-30T12:00:00Z", "2026-10-30", 0, "2026-10-30T11:45"),
    ] {
        let time = OffsetDateTime::parse(now_text, &Rfc3339).unwrap();
        let mut v = response();
        v["utc_offset_seconds"] = json!(offset);
        v["current"]["time"] = json!(valid);
        if offset == 0 {
            v["timezone"] = json!("GMT");
        }
        let result = preview(&request(day), time, |_| Ok(body(v.clone()))).unwrap();
        assert_eq!(result.fetched.source.product, "current-weather");
        if offset == 0 {
            assert_eq!(result.fetched.timezone, "UTC");
        }
    }
}

#[test]
#[ignore = "Explicit HTTPS verification with synthetic coordinates; never run by default"]
fn live_forecast_contract_synthetic_coordinates() {
    let time = OffsetDateTime::now_utc();
    let http = client(Duration::from_secs(15)).unwrap();
    let req = request(&time.date().to_string());
    let (status, bytes) = get(&http, forecast_url(&req, true).unwrap()).unwrap();
    let meta = metadata(status, &bytes).unwrap();
    let today = time
        .to_offset(UtcOffset::from_whole_seconds(meta.utc_offset_seconds).unwrap())
        .date();
    for day in [today, today - time::Duration::days(1)] {
        let result = preview(&request(&day.to_string()), time, |url| get(&http, url)).unwrap();
        assert_eq!(result.fetched.source.data_kind, DataKind::Forecast);
        if day == today {
            assert!(result.fetched.valid_at.is_some());
            assert!(result.fetched.temperature_mean_c.is_none());
        } else {
            assert!(result.fetched.valid_at.is_none());
            assert!(result.fetched.temperature_mean_c.is_some());
        }
        println!(
            "{}: product={}, timezone={}, valid_at={:?}, retrieved_at={}",
            day,
            result.fetched.source.product,
            result.fetched.timezone,
            result.fetched.valid_at,
            result.fetched.retrieved_at
        );
    }
}

#[test]
fn current_and_recent_responses_share_service_error_classification() {
    for (status, body, expected) in [
        (
            200,
            br#"{"error":true,"reason":"The service is overloaded"}"#.as_slice(),
            Error::ServiceUnavailable,
        ),
        (
            400,
            br#"{"error":true,"reason":"Too many requests"}"#.as_slice(),
            Error::RateLimited,
        ),
        (429, b"".as_slice(), Error::RateLimited),
        (503, b"not JSON".as_slice(), Error::ServiceUnavailable),
        (403, b"".as_slice(), Error::Http),
        (
            200,
            br#"{"error":true,"reason":"bad request"}"#.as_slice(),
            Error::Api,
        ),
    ] {
        assert_eq!(metadata(status, body).err().unwrap(), expected);
        assert_eq!(
            preview(&request("2026-10-30"), now(), |_| Ok((
                status,
                body.to_vec()
            )))
            .err()
            .unwrap(),
            expected
        );
    }
}
