use super::*;
use serde_json::{Value, json};

fn manual() -> Value {
    json!({
        "period": {"kind": "day", "date": "2026-10-17"},
        "location": null, "timezone": null,
        "description": "  Kallt vid sjön — rain & blåst.\nOförändrat!  ",
        "temperatureC": 0.0, "windSpeedMs": null, "precipitationMm": 0.0,
        "weatherCode": null
    })
}
fn fetched() -> Value {
    json!({
        "period": {"kind": "day", "date": "2026-10-16"},
        "location": {"name": "  Min plats  ", "coordinates": {"latitude": 59.33, "longitude": 18.07}},
        "timezone": "Europe/Stockholm",
        "source": {
            "provider": "open-meteo", "product": "historical-weather", "model": "ERA5",
            "dataKind": "reanalysis", "sampledCoordinates": {"latitude": 59.25, "longitude": 18.0},
            "attribution": "Weather data by Open-Meteo", "license": "CC-BY-4.0",
            "licenseUrl": "https://creativecommons.org/licenses/by/4.0/"
        },
        "retrievedAt": "2026-10-23T09:30:00.123Z",
        "temperatureMeanC": 7.2, "temperatureMinC": 4.1, "temperatureMaxC": 10.3,
        "precipitationSumMm": 1.8, "windSpeedMaxMs": 5.2, "weatherCode": 61
    })
}
fn envelope(manual: Value, fetched: Value) -> Value {
    json!({"schemaVersion": 1, "entryId": "entry-1", "manual": manual, "fetched": fetched})
}
fn parse(value: &Value) -> Result<WeatherContract, WeatherError> {
    WeatherContract::from_json(&value.to_string())
}
fn assert_roundtrip(value: &Value) {
    let original = value.to_string();
    let weather = WeatherContract::from_json(&original).unwrap();
    let serialized = weather.to_json().unwrap();
    assert_eq!(serde_json::from_str::<Value>(&serialized).unwrap(), *value);
    assert_eq!(WeatherContract::from_json(&serialized).unwrap(), weather);
    assert_eq!(
        serde_json::from_str::<WeatherContract>(&original).unwrap(),
        weather
    );
    assert_eq!(value.to_string(), original, "parsing must not mutate input");
}

#[test]
fn manual_only_preserves_description_exactly_without_location_or_timezone() {
    let value = envelope(manual(), Value::Null);
    assert_roundtrip(&value);
    let parsed = parse(&value).unwrap();
    assert_eq!(
        parsed.manual.unwrap().description.as_deref(),
        manual()["description"].as_str()
    );
}
#[test]
fn fetched_only_preserves_source_request_and_grid_coordinates() {
    assert_roundtrip(&envelope(Value::Null, fetched()));
}
#[test]
fn both_sources_keep_independent_dates_and_values() {
    let value = envelope(manual(), fetched());
    assert_roundtrip(&value);
    let parsed = parse(&value).unwrap();
    assert_ne!(
        parsed.manual.as_ref().unwrap().period,
        parsed.fetched.as_ref().unwrap().period
    );
    assert!(value.get("createdAt").is_none());
}
#[test]
fn absent_values_are_not_zero_and_no_weather_is_allowed() {
    let zero = parse(&envelope(manual(), Value::Null)).unwrap();
    assert_eq!(zero.manual.as_ref().unwrap().temperature_c.unwrap().0, 0.0);
    let mut missing = manual();
    missing["temperatureC"] = Value::Null;
    missing["precipitationMm"] = Value::Null;
    let missing = envelope(missing, Value::Null);
    assert_roundtrip(&missing);
    let parsed = parse(&missing).unwrap();
    assert!(parsed.manual.unwrap().temperature_c.is_none());
    assert_roundtrip(&envelope(Value::Null, Value::Null));
    // Omitted optional fields are semantically absent and serialize as null.
    let sparse = json!({"schemaVersion":1,"entryId":"entry-1","manual":{"period":{"kind":"day","date":"2024-02-29"}}});
    assert!(parse(&sparse).is_ok());
}
#[test]
fn invalid_temperatures_and_nonfinite_numbers_are_rejected() {
    for value in [-273.15001, f64::NEG_INFINITY, f64::INFINITY, f64::NAN] {
        assert_eq!(
            Celsius::try_from(value),
            Err(WeatherError::InvalidTemperature)
        );
    }
    for source in ["manual", "fetched"] {
        let mut value = envelope(manual(), fetched());
        let field = if source == "manual" {
            "temperatureC"
        } else {
            "temperatureMinC"
        };
        value[source][field] = json!(-274);
        assert!(parse(&value).is_err());
    }
    let malformed = r#"{"schemaVersion":1,"entryId":"x","manual":{"period":{"kind":"day","date":"2026-10-17"},"temperatureC":1e999}}"#;
    assert!(WeatherContract::from_json(malformed).is_err());
    assert!(Celsius::try_from(-273.15).is_ok());
}
#[test]
fn canonical_celsius_and_fahrenheit_display_are_language_independent() {
    for (c, f) in [(0.0, 32.0), (100.0, 212.0), (-40.0, -40.0), (20.0, 68.0)] {
        let value = Celsius::from_display(f, TemperatureUnit::Fahrenheit).unwrap();
        assert!((value.display(TemperatureUnit::Celsius).unwrap() - c).abs() < 1e-10);
        assert!((value.display(TemperatureUnit::Fahrenheit).unwrap() - f).abs() < 1e-10);
        assert_eq!(serde_json::to_value(value).unwrap(), json!(c));
    }
    assert_eq!(
        Celsius::from_display(-459.67, TemperatureUnit::Fahrenheit).unwrap(),
        Celsius::try_from(-273.15).unwrap()
    );
    assert!(Celsius::from_display(-459.67001, TemperatureUnit::Fahrenheit).is_err());
    assert!(Celsius::from_display(-500.0, TemperatureUnit::Fahrenheit).is_err());
    assert!(Celsius::from_display(f64::NAN, TemperatureUnit::Celsius).is_err());
    assert!(
        Celsius::try_from(f64::MAX)
            .unwrap()
            .display(TemperatureUnit::Fahrenheit)
            .is_err()
    );
}
#[test]
fn invalid_coordinate_ranges_and_missing_fetched_coordinates_are_rejected() {
    for (lat, lon) in [
        (90.01, 18.0),
        (-90.01, 18.0),
        (59.0, 180.01),
        (59.0, -180.01),
    ] {
        let mut value = envelope(Value::Null, fetched());
        value["fetched"]["location"]["coordinates"] = json!({"latitude":lat,"longitude":lon});
        assert!(parse(&value).is_err());
        value["fetched"]["location"]["coordinates"] = json!({"latitude":0.0,"longitude":0.0});
        value["fetched"]["source"]["sampledCoordinates"] = json!({"latitude":lat,"longitude":lon});
        assert!(parse(&value).is_err());
    }
    let mut value = envelope(Value::Null, fetched());
    value["fetched"]["location"]["coordinates"] = Value::Null;
    assert!(parse(&value).is_err());
    let mut named_only = envelope(manual(), Value::Null);
    named_only["manual"]["location"] = json!({"name":"Sjön","coordinates":null});
    assert_roundtrip(&named_only);
    for coordinates in [
        Coordinates {
            latitude: f64::NAN,
            longitude: 0.0,
        },
        Coordinates {
            latitude: 0.0,
            longitude: f64::INFINITY,
        },
    ] {
        assert!(coordinates.validate().is_err());
    }
}
#[test]
fn invalid_dates_intervals_and_unqualified_times_are_rejected() {
    for period in [
        json!({"kind":"day","date":"2026-02-29"}),
        json!({"kind":"day","date":"2026-13-01"}),
        json!({"kind":"day","date":"2026-1-01"}),
        json!({"kind":"hour","start":"2026-10-17T14:00:00","endExclusive":"2026-10-17T15:00:00"}),
        json!({"kind":"hour","start":"2026-10-17T14:00:00Z","endExclusive":"2026-10-17T14:00:00Z"}),
        json!({"kind":"hour","start":"2026-10-17T14:00:00Z","endExclusive":"2026-10-17T13:00:00Z"}),
        json!({"kind":"hour","start":"2026-10-17T14:00:00Z","endExclusive":"2026-10-17T16:00:00Z"}),
    ] {
        let mut value = envelope(manual(), fetched());
        value["manual"]["period"] = period.clone();
        assert!(parse(&value).is_err());
        value["manual"] = Value::Null;
        value["fetched"]["period"] = period;
        assert!(parse(&value).is_err());
    }
}
#[test]
fn hourly_offsets_and_midnight_boundaries_roundtrip_without_normalization() {
    for period in [
        json!({"kind":"hour","start":"2026-10-17T23:30:00+02:00","endExclusive":"2026-10-18T00:30:00+02:00"}),
        json!({"kind":"hour","start":"2026-10-25T02:00:00+02:00","endExclusive":"2026-10-25T02:00:00+01:00"}),
    ] {
        let mut value = envelope(manual(), fetched());
        value["manual"]["period"] = period.clone();
        value["fetched"]["period"] = period;
        assert_roundtrip(&value);
    }
}
#[test]
fn unsupported_versions_unknown_fields_and_invalid_json_do_not_silently_disappear() {
    for version in [json!(0), json!(2), json!(-1), json!(1.5), json!("1")] {
        let mut value = envelope(manual(), fetched());
        value["schemaVersion"] = version;
        let original = value.to_string();
        assert!(WeatherContract::from_json(&original).is_err());
        assert_eq!(value.to_string(), original);
        assert!(serde_json::from_str::<WeatherContract>(&original).is_err());
    }
    for path in [
        "",
        "/manual",
        "/manual/period",
        "/fetched",
        "/fetched/source",
        "/fetched/location",
        "/fetched/location/coordinates",
    ] {
        let mut value = envelope(manual(), fetched());
        let object = if path.is_empty() {
            &mut value
        } else {
            value.pointer_mut(path).unwrap()
        };
        object["future"] = json!("must not be lost");
        assert!(parse(&value).is_err(), "{path}");
    }
    for malformed in [
        "{",
        "null",
        "[]",
        r#"{"schemaVersion":1,"schemaVersion":2,"entryId":"x"}"#,
    ] {
        assert!(WeatherContract::from_json(malformed).is_err());
    }
}
#[test]
fn invalid_metrics_ranges_source_and_retrieval_context_are_rejected() {
    for (path, invalid) in [
        ("/entryId", json!("  ")),
        ("/manual/windSpeedMs", json!(-1)),
        ("/manual/precipitationMm", json!(-1)),
        ("/manual/timezone", json!("not a zone")),
        ("/fetched/timezone", json!("")),
        ("/fetched/retrievedAt", json!("2026-10-23")),
        ("/fetched/retrievedAt", json!("2026-10-23T12:30:00+02:00")),
        ("/fetched/windSpeedMaxMs", json!(-1)),
        ("/fetched/precipitationSumMm", json!(-1)),
        ("/fetched/temperatureMinC", json!(20)),
        ("/fetched/temperatureMaxC", json!(5)),
        ("/fetched/source/product", json!("")),
        ("/fetched/source/model", json!("  ")),
        ("/fetched/source/provider", json!("unknown")),
        ("/fetched/source/dataKind", json!("personalObservation")),
        ("/fetched/source/licenseUrl", json!("javascript:alert(1)")),
    ] {
        let mut value = envelope(manual(), fetched());
        *value.pointer_mut(path).unwrap() = invalid;
        assert!(parse(&value).is_err(), "{path}");
    }
}
#[test]
fn weather_codes_are_numeric_language_free_and_checked() {
    for code in [0, 3, 45, 61, 86, 97, 99] {
        let mut value = envelope(manual(), fetched());
        value["manual"]["weatherCode"] = json!(code);
        value["fetched"]["weatherCode"] = json!(code);
        assert_roundtrip(&value);
    }
    for code in [json!(4), json!(-1), json!(256), json!(1.5), json!("rain")] {
        let mut value = envelope(manual(), fetched());
        value["fetched"]["weatherCode"] = code;
        assert!(parse(&value).is_err());
    }
    let value = envelope(manual(), fetched());
    assert!(value.get("language").is_none());
    assert!(value.get("temperatureUnit").is_none());
    assert!(value["manual"].get("temperatureF").is_none());
}

#[test]
fn numeric_precision_optional_provenance_and_unknown_preferences_are_preserved_or_rejected() {
    let mut value = envelope(manual(), fetched());
    value["manual"]["temperatureC"] = json!(0.12345678901234566);
    value["manual"]["windSpeedMs"] = json!(0.0);
    value["manual"]["precipitationMm"] = json!(-0.0);
    value["fetched"]["source"]["model"] = Value::Null;
    value["fetched"]["source"]["sampledCoordinates"] = Value::Null;
    value["fetched"]["source"]["licenseUrl"] = Value::Null;
    assert_roundtrip(&value);
    for key in ["language", "temperatureUnit", "temperatureF"] {
        let mut extra = value.clone();
        extra[key] = json!("not part of the stored contract");
        assert!(parse(&extra).is_err());
    }
    for value in [f64::NAN, f64::INFINITY, -1.0] {
        assert_eq!(
            nonnegative(Some(value), WeatherError::InvalidWind),
            Err(WeatherError::InvalidWind)
        );
        assert_eq!(
            nonnegative(Some(value), WeatherError::InvalidPrecipitation),
            Err(WeatherError::InvalidPrecipitation)
        );
    }
    let mut version = envelope(Value::Null, Value::Null);
    version["schemaVersion"] = json!(2);
    assert_eq!(parse(&version), Err(WeatherError::UnsupportedVersion));
}
