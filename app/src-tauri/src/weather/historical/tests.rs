use super::*;
use serde_json::{Value, json};

pub(super) fn now() -> OffsetDateTime {
    OffsetDateTime::parse("2026-10-30T12:00:00Z", &Rfc3339).unwrap()
}
pub(super) fn request() -> PreviewRequest {
    PreviewRequest {
        date: "2020-06-01".into(),
        place_name: "Synthetic lake".into(),
        latitude: 59.3,
        longitude: 18.1,
    }
}
pub(super) fn response() -> Value {
    json!({"latitude":59.25,"longitude":18.0,"timezone":"Europe/Stockholm","utc_offset_seconds":7200,
        "daily_units":{"time":"iso8601","temperature_2m_mean":"°C","temperature_2m_min":"°C","temperature_2m_max":"°C","precipitation_sum":"mm","wind_speed_10m_max":"m/s","weather_code":"wmo code","sunrise":"iso8601","sunset":"iso8601"},
        "daily":{"time":["2020-06-01"],"temperature_2m_mean":[12.5],"temperature_2m_min":[5.0],"temperature_2m_max":[20.0],"precipitation_sum":[0.04],"wind_speed_10m_max":[0.0],"weather_code":[0],"sunrise":["2020-06-01T03:45"],"sunset":["2020-06-01T21:50"]}})
}
fn run(value: Value) -> Result<Preview, Error> {
    preview_with(&request(), now(), |_| {
        Ok((200, serde_json::to_vec(&value).unwrap()))
    })
}
fn error(result: Result<Preview, Error>) -> Error {
    result.err().expect("must reject")
}

#[test]
fn full_preview_uses_fixed_era5_daily_request_and_existing_contract() {
    let result = preview_with(&request(), now(), |url| {
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("archive-api.open-meteo.com"));
        let query: std::collections::BTreeMap<_, _> = url.query_pairs().into_owned().collect();
        for (key, value) in [
            ("models", "era5"),
            ("timezone", "auto"),
            ("start_date", "2020-06-01"),
            ("end_date", "2020-06-01"),
            ("wind_speed_unit", "ms"),
            ("temperature_unit", "celsius"),
            ("precipitation_unit", "mm"),
            ("daily", DAILY),
        ] {
            assert_eq!(query[key], value);
        }
        Ok((200, serde_json::to_vec(&response()).unwrap()))
    })
    .unwrap();
    let wire = serde_json::to_value(result).unwrap();
    assert_eq!(wire["fetched"]["temperatureMeanC"], 12.5);
    assert_eq!(wire["fetched"]["windSpeedMaxMs"], 0.0);
    assert_eq!(wire["fetched"]["weatherCode"], 0);
    assert_eq!(wire["fetched"]["precipitationSumMm"], 0.04);
    assert_eq!(wire["fetched"]["location"]["coordinates"]["latitude"], 59.3);
    assert_eq!(
        wire["fetched"]["source"]["sampledCoordinates"]["latitude"],
        59.25
    );
    assert_eq!(wire["fetched"]["source"]["model"], "ERA5");
    assert_eq!(wire["fetched"]["retrievedAt"], "2026-10-30T12:00:00Z");
}
#[test]
fn missing_and_null_values_remain_absent_without_inventing_zero() {
    let mut value = response();
    value["daily"]
        .as_object_mut()
        .unwrap()
        .remove("temperature_2m_mean");
    value["daily"]["precipitation_sum"] = json!([null]);
    value["daily"]["sunrise"] = json!([null]);
    let preview = serde_json::to_value(run(value).unwrap()).unwrap();
    assert!(preview["fetched"]["temperatureMeanC"].is_null());
    assert!(preview["fetched"]["precipitationSumMm"].is_null());
    assert!(preview["fetched"].get("sunrise").is_none());
    assert_eq!(preview["fetched"]["windSpeedMaxMs"], 0.0);
}
#[test]
fn empty_weather_and_incomplete_structures_are_distinct_errors() {
    let mut value = response();
    for (key, values) in value["daily"].as_object_mut().unwrap() {
        if key != "time" {
            *values = json!([null]);
        }
    }
    assert_eq!(error(run(value)), Error::NoData);
    for path in ["daily", "daily_units", "timezone", "utc_offset_seconds"] {
        let mut value = response();
        value.as_object_mut().unwrap().remove(path);
        assert_eq!(error(run(value)), Error::InvalidResponse);
    }
    for array in [json!([]), json!([1, 2])] {
        let mut value = response();
        value["daily"]["temperature_2m_mean"] = array;
        assert_eq!(error(run(value)), Error::InvalidResponse);
    }
}
#[test]
fn invalid_inputs_fail_before_http_including_recent_and_future_dates() {
    for (date, expected) in [
        ("2020-02-30", Error::InvalidDate),
        ("2020-1-01", Error::InvalidDate),
        ("1939-12-31", Error::DateUnavailable),
        ("2026-10-26", Error::DateUnavailable),
        ("2026-10-30", Error::DateUnavailable),
        ("2027-01-01", Error::DateUnavailable),
    ] {
        let mut r = request();
        r.date = date.into();
        assert_eq!(
            error(preview_with(&r, now(), |_| panic!("no request"))),
            expected
        );
    }
    for (latitude, longitude) in [
        (91.0, 0.0),
        (0.0, -181.0),
        (f64::NAN, 0.0),
        (0.0, f64::INFINITY),
    ] {
        let mut r = request();
        r.latitude = latitude;
        r.longitude = longitude;
        assert_eq!(
            error(preview_with(&r, now(), |_| panic!("no request"))),
            Error::InvalidCoordinates
        );
    }
    let mut r = request();
    r.place_name = " ".into();
    assert_eq!(
        error(preview_with(&r, now(), |_| panic!("no request"))),
        Error::InvalidPlace
    );
    for (latitude, longitude) in [(90.0, 180.0), (-90.0, -180.0), (0.0, 0.0)] {
        let mut r = request();
        r.latitude = latitude;
        r.longitude = longitude;
        validate_request(&r, now()).unwrap();
    }
}
#[test]
fn dates_at_availability_boundaries_are_valid() {
    for date in ["1940-01-01", "2024-02-29", "2026-10-25"] {
        let mut r = request();
        r.date = date.into();
        validate_request(&r, now()).unwrap();
    }
}
#[test]
fn wrong_values_units_dates_and_extreme_offsets_are_rejected() {
    for (path, value) in [
        ("/daily/precipitation_sum", json!([-1])),
        ("/daily/temperature_2m_min", json!([30])),
        ("/daily/temperature_2m_mean", json!([-300])),
        ("/daily/weather_code", json!([255])),
        ("/daily/weather_code", json!([1.5])),
        ("/latitude", json!(91)),
        ("/timezone", json!("../invalid")),
        ("/utc_offset_seconds", json!(i32::MIN)),
        ("/daily/time", json!(["2020-06-02"])),
        ("/daily_units/wind_speed_10m_max", json!("km/h")),
    ] {
        let mut response = response();
        *response.pointer_mut(path).unwrap() = value;
        assert_eq!(error(run(response)), Error::InvalidResponse, "{path}");
    }
}
#[test]
fn solar_times_preserve_api_offset_and_reject_malformed_or_wrong_day() {
    let preview = run(response()).unwrap();
    assert_eq!(preview.fetched.timezone, "Europe/Stockholm");
    assert_eq!(
        preview.fetched.sunrise.as_deref(),
        Some("2020-06-01T03:45:00+02:00")
    );
    assert_eq!(
        preview.fetched.sunset.as_deref(),
        Some("2020-06-01T21:50:00+02:00")
    );
    for sun in ["2020-06-02T03:45", "broken", "2020-06-01T23:00"] {
        let mut value = response();
        value["daily"]["sunrise"] = json!([sun]);
        assert_eq!(error(run(value)), Error::InvalidResponse);
    }
}
#[test]
fn http_api_network_and_timeout_errors_do_not_become_no_data() {
    for (status, expected) in [
        (400, Error::Api),
        (429, Error::RateLimited),
        (500, Error::ServiceUnavailable),
        (302, Error::Http),
    ] {
        assert_eq!(
            error(preview_with(&request(), now(), |_| Ok((status, vec![])))),
            expected
        );
    }
    for failure in [Error::Timeout, Error::Network] {
        let expected = if failure == Error::Timeout {
            Error::Timeout
        } else {
            Error::Network
        };
        assert_eq!(
            error(preview_with(&request(), now(), |_| Err(failure))),
            expected
        );
    }
    assert_eq!(
        error(run(json!({"error":true,"reason":"bad request"}))),
        Error::Api
    );
    for body in [b"not JSON".to_vec(), vec![b' '; BODY_LIMIT + 1]] {
        assert_eq!(
            error(preview_with(&request(), now(), |_| Ok((200, body)))),
            Error::InvalidResponse
        );
    }
}
#[test]
fn generation_is_checked_before_network_and_again_after_response() {
    assert!(matches!(
        preview_for_generation(&DatabaseState::default(), 1, &request(), now(), |_| panic!(
            "no HTTP before Ready"
        )),
        Err(Error::DatabaseUnavailable)
    ));
    let state = DatabaseState::ready_for_test();
    assert!(matches!(
        preview_for_generation(&state, 2, &request(), now(), |_| panic!("no HTTP")),
        Err(Error::GenerationConflict)
    ));
    let result = preview_for_generation(&state, 1, &request(), now(), |_| {
        state.maintenance().unwrap().advance_generation().unwrap();
        Ok((200, serde_json::to_vec(&response()).unwrap()))
    });
    assert!(matches!(result, Err(Error::GenerationConflict)));
}
#[test]
fn preview_preserves_synthetic_journal_weather_and_schema_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.sqlite");
    crate::database::create_current_database(&path).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "INSERT INTO journal_entries VALUES('entry','2020-06-01','Synthetic','{}',1,'c','u')",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO journal_weather VALUES('entry',1,?1,NULL,2)",
        [r#"{"period":{"kind":"day","date":"2020-06-01"},"temperatureC":17}"#],
    )
    .unwrap();
    db.close().unwrap();
    let before = std::fs::read(&path).unwrap();
    let state = DatabaseState::ready_for_test();
    let preview = preview_for_generation(&state, 1, &request(), now(), |_| {
        Ok((200, serde_json::to_vec(&response()).unwrap()))
    })
    .unwrap();
    assert_eq!(preview.generation, 1);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(state.operation(Some(1)).is_ok());
}

fn mock_server(
    status: u16,
    body: Vec<u8>,
    delay: Duration,
) -> (reqwest::Url, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url =
        reqwest::Url::parse(&format!("http://{}/mock", listener.local_addr().unwrap())).unwrap();
    let task = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        while request.len() < 8192 && !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            if stream.read(&mut byte).unwrap_or(0) == 0 {
                return;
            }
            request.push(byte[0]);
        }
        assert!(request.starts_with(b"GET /mock HTTP/1.1"));
        std::thread::sleep(delay);
        let header = format!(
            "HTTP/1.1 {status} Mock\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        if stream.write_all(header.as_bytes()).is_ok() {
            let _ = stream.write_all(&body);
        }
    });
    (url, task)
}

#[test]
fn real_http_transport_handles_mocked_success_http_failure_and_resource_limit() {
    // Only the test client permits loopback HTTP. Production is fixed to HTTPS.
    let http = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    for (status, body, expected) in [
        (200, serde_json::to_vec(&response()).unwrap(), None),
        (400, b"{}".to_vec(), Some(Error::Api)),
        (
            400,
            br#"{"error":true,"reason":"The service is overloaded"}"#.to_vec(),
            Some(Error::ServiceUnavailable),
        ),
        (429, b"not JSON".to_vec(), Some(Error::RateLimited)),
        (
            502,
            b"<html>Bad Gateway</html>".to_vec(),
            Some(Error::ServiceUnavailable),
        ),
        (
            503,
            vec![b' '; BODY_LIMIT + 1],
            Some(Error::ServiceUnavailable),
        ),
        (503, b"{}".to_vec(), Some(Error::ServiceUnavailable)),
        (
            200,
            vec![b' '; BODY_LIMIT + 1],
            Some(Error::InvalidResponse),
        ),
    ] {
        let (url, server) = mock_server(status, body, Duration::ZERO);
        let result = preview_with(&request(), now(), |_| get(&http, url));
        match expected {
            None => {
                result.unwrap();
            }
            Some(expected) => assert_eq!(error(result), expected),
        }
        server.join().unwrap();
    }
}
#[test]
fn real_http_timeout_is_reported_and_production_rejects_plain_http() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url =
        reqwest::Url::parse(&format!("http://{}/mock", listener.local_addr().unwrap())).unwrap();
    let (release, wait) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        // Keep the connection open without a response until the client finishes.
        // A fixed server delay races the timeout timer on a busy runner.
        let _ = wait.recv();
        drop(stream);
    });
    let http = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_millis(80))
        .build()
        .unwrap();
    let result = preview_with(&request(), now(), |_| get(&http, url));
    release.send(()).unwrap();
    server.join().unwrap();
    assert_eq!(error(result), Error::Timeout);
    assert!(
        client(Duration::from_secs(1))
            .unwrap()
            .get("http://127.0.0.1:1")
            .send()
            .is_err()
    );
}
#[test]
fn malformed_json_duplicate_fields_and_invalid_sun_arrays_are_rejected() {
    let body = serde_json::to_string(&response()).unwrap();
    let duplicate = body.replacen(
        "\"latitude\":59.25",
        "\"latitude\":59.25,\"latitude\":59.25",
        1,
    );
    assert_eq!(
        error(preview_with(&request(), now(), |_| Ok((
            200,
            duplicate.into_bytes()
        )))),
        Error::InvalidResponse
    );
    let mut invalid = response();
    invalid["daily"]["sunrise"] = json!([]);
    assert_eq!(error(run(invalid)), Error::InvalidResponse);
}

#[test]
fn provider_failures_are_classified_without_changing_success_decoding() {
    for status in [200, 400, 503] {
        let body = br#"{"error":true,"reason":"The service is overloaded"}"#;
        assert_eq!(
            response_error(status, body),
            Some(Error::ServiceUnavailable)
        );
        assert_eq!(
            error(preview_with(&request(), now(), |_| Ok((
                status,
                body.to_vec()
            )))),
            Error::ServiceUnavailable
        );
    }
    for reason in [
        "Too many requests",
        "Hourly API request limit exceeded. Please try again later.",
        "Rate limit exceeded",
    ] {
        let body = serde_json::to_vec(&json!({"error":true,"reason":reason})).unwrap();
        assert_eq!(response_error(400, &body), Some(Error::RateLimited));
    }
    for (status, body, expected) in [
        (429, b"".as_slice(), Error::RateLimited),
        (500, b"invalid JSON".as_slice(), Error::ServiceUnavailable),
        (504, b"".as_slice(), Error::ServiceUnavailable),
        (403, b"".as_slice(), Error::Http),
        (
            400,
            br#"{"error":true,"reason":"Unknown parameter"}"#.as_slice(),
            Error::Api,
        ),
        (200, br#"{"error":true}"#.as_slice(), Error::Api),
    ] {
        assert_eq!(response_error(status, body), Some(expected));
    }
    assert_eq!(
        response_error(
            200,
            br#"{"error":false,"reason":"The service is overloaded"}"#
        ),
        None
    );
    assert_eq!(response_error(200, br#"{"reason":"rate limit"}"#), None);
    assert!(run(response()).is_ok());
}

#[test]
fn disconnected_http_peer_remains_a_network_failure() {
    use std::io::Read;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url =
        reqwest::Url::parse(&format!("http://{}/mock", listener.local_addr().unwrap())).unwrap();
    let http = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        // Keep ownership of the port and close only after the request arrives.
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            assert!(request.len() < 8192);
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        assert!(request.starts_with(b"GET /mock HTTP/1.1"));
    });
    assert_eq!(get(&http, url).unwrap_err(), Error::Network);
    server.join().unwrap();
}
