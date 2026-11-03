//! Explicit ERA5 daily preview and separately confirmed, conflict-checked saving.
mod recent;
pub(crate) mod saving;
use super::{
    Celsius, Coordinates, DataKind, FetchedWeather, Location, Period, Provider, Source, WeatherCode,
};
use crate::database_state::{DatabaseState, Snapshot};
use serde::{Deserialize, Serialize};
use std::{io::Read, time::Duration};
use time::{
    Date, OffsetDateTime, PrimitiveDateTime, UtcOffset, format_description,
    format_description::well_known::Rfc3339,
};

const ENDPOINT: &str = "https://archive-api.open-meteo.com/v1/archive";
const DAILY: &str = "temperature_2m_mean,temperature_2m_min,temperature_2m_max,precipitation_sum,wind_speed_10m_max,weather_code,sunrise,sunset";
const BODY_LIMIT: usize = 64 * 1024;

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Error {
    InvalidCoordinates,
    InvalidDate,
    InvalidPlace,
    DateUnavailable,
    Timeout,
    Network,
    Http,
    ServiceUnavailable,
    RateLimited,
    Api,
    InvalidResponse,
    NoData,
    GenerationConflict,
    DatabaseUnavailable,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PreviewRequest {
    date: String,
    place_name: String,
    latitude: f64,
    longitude: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Preview {
    fetched: FetchedWeather,
}

#[derive(Deserialize)]
struct Response {
    latitude: f64,
    longitude: f64,
    timezone: String,
    utc_offset_seconds: i32,
    daily: Daily,
    daily_units: std::collections::BTreeMap<String, String>,
}
#[derive(Deserialize)]
struct Daily {
    time: Vec<String>,
    temperature_2m_mean: Option<Vec<Option<f64>>>,
    temperature_2m_min: Option<Vec<Option<f64>>>,
    temperature_2m_max: Option<Vec<Option<f64>>>,
    precipitation_sum: Option<Vec<Option<f64>>>,
    wind_speed_10m_max: Option<Vec<Option<f64>>>,
    weather_code: Option<Vec<Option<u8>>>,
    sunrise: Option<Vec<Option<String>>>,
    sunset: Option<Vec<Option<String>>>,
}

fn validate_request(request: &PreviewRequest, now: OffsetDateTime) -> Result<(), Error> {
    Coordinates {
        latitude: request.latitude,
        longitude: request.longitude,
    }
    .validate()
    .map_err(|_| Error::InvalidCoordinates)?;
    if request.place_name.trim().is_empty() || request.place_name.len() > 512 {
        return Err(Error::InvalidPlace);
    }
    if request.date.len() != 10 {
        return Err(Error::InvalidDate);
    }
    let format = format_description::parse_borrowed::<2>("[year]-[month]-[day]").unwrap();
    let date = Date::parse(&request.date, &format).map_err(|_| Error::InvalidDate)?;
    if request.date.len() != 10 || date.to_string() != request.date {
        return Err(Error::InvalidDate);
    }
    // ERA5 is published with roughly five days of delay. A valid date still
    // needs response validation; publication delays can vary.
    if request.date.as_str() < "1940-01-01"
        || date > (now.to_offset(UtcOffset::UTC) - time::Duration::days(5)).date()
    {
        return Err(Error::DateUnavailable);
    }
    Ok(())
}

fn url(request: &PreviewRequest) -> Result<reqwest::Url, Error> {
    let mut url = reqwest::Url::parse(ENDPOINT).map_err(|_| Error::Network)?;
    url.query_pairs_mut().extend_pairs([
        ("latitude", request.latitude.to_string()),
        ("longitude", request.longitude.to_string()),
        ("start_date", request.date.clone()),
        ("end_date", request.date.clone()),
        ("models", "era5".into()),
        ("timezone", "auto".into()),
        ("temperature_unit", "celsius".into()),
        ("wind_speed_unit", "ms".into()),
        ("precipitation_unit", "mm".into()),
        ("timeformat", "iso8601".into()),
        ("daily", DAILY.into()),
    ]);
    Ok(url)
}

fn client(timeout: Duration) -> Result<reqwest::blocking::Client, Error> {
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .https_only(true)
        .user_agent("MyNatureObservations/0.1")
        .build()
        .map_err(|_| Error::Network)
}
fn http_error(error: reqwest::Error) -> Error {
    let classified = if error.is_timeout() {
        Error::Timeout
    } else {
        Error::Network
    };
    #[cfg(debug_assertions)]
    eprintln!("Open-Meteo transport failure: {}", error.without_url());
    classified
}

// Shared by archive, recent-day and current responses. Provider error reasons are
// interpreted only inside an explicit API error, never in successful weather data.
fn response_error(status: u16, body: &[u8]) -> Option<Error> {
    let raw: Option<serde_json::Value> = if body.len() <= BODY_LIMIT {
        serde_json::from_slice(body).ok()
    } else {
        None
    };
    let api_error = raw
        .as_ref()
        .is_some_and(|value| value.get("error").and_then(|v| v.as_bool()) == Some(true));
    let reason = if api_error {
        raw.as_ref()
            .and_then(|value| value.get("reason"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
    } else {
        ""
    };
    let lower = reason.to_ascii_lowercase();
    let result = if status == 429 {
        Some(Error::RateLimited)
    } else if (500..600).contains(&status) {
        Some(Error::ServiceUnavailable)
    } else if api_error
        && (lower.contains("too many requests")
            || lower.contains("rate limit")
            || lower.contains("api request limit exceeded"))
    {
        Some(Error::RateLimited)
    } else if api_error
        && (lower.contains("service is overloaded")
            || lower.contains("service unavailable")
            || lower.contains("temporarily unavailable"))
    {
        Some(Error::ServiceUnavailable)
    } else if status == 400 || api_error {
        Some(Error::Api)
    } else if !(200..300).contains(&status) {
        Some(Error::Http)
    } else {
        None
    };
    #[cfg(debug_assertions)]
    if let Some(error) = &result {
        // Bounded and escaped provider reason; no request URL or successful payload.
        let detail: String = reason.chars().take(240).collect();
        eprintln!("Open-Meteo response: HTTP {status}, {error:?}, reason={detail:?}");
    }
    result
}

fn get(client: &reqwest::blocking::Client, url: reqwest::Url) -> Result<(u16, Vec<u8>), Error> {
    let response = client.get(url).send().map_err(http_error)?;
    let status = response.status().as_u16();
    let success = (200..300).contains(&status);
    if response
        .content_length()
        .is_some_and(|size| size > BODY_LIMIT as u64)
    {
        return if success {
            Err(Error::InvalidResponse)
        } else {
            Ok((status, Vec::new()))
        };
    }
    let mut body = Vec::new();
    let read = response.take(BODY_LIMIT as u64 + 1).read_to_end(&mut body);
    if let Err(error) = read {
        #[cfg(debug_assertions)]
        eprintln!(
            "Open-Meteo body read failed: HTTP {status}, {:?}",
            error.kind()
        );
        // A received HTTP failure remains a service/API failure even when its
        // optional diagnostic body cannot be read.
        if !success {
            return Ok((status, Vec::new()));
        }
        return Err({
            if error.kind() == std::io::ErrorKind::TimedOut
                || error
                    .get_ref()
                    .and_then(|e| e.downcast_ref::<reqwest::Error>())
                    .is_some_and(|e| e.is_timeout())
            {
                Error::Timeout
            } else {
                Error::Network
            }
        });
    }
    if body.len() > BODY_LIMIT {
        return if success {
            Err(Error::InvalidResponse)
        } else {
            Ok((status, Vec::new()))
        };
    }
    Ok((status, body))
}

fn one<T>(values: Option<Vec<Option<T>>>) -> Result<Option<T>, Error> {
    match values {
        None => Ok(None),
        Some(mut values) if values.len() == 1 => Ok(values.pop().unwrap()),
        _ => Err(Error::InvalidResponse),
    }
}
fn sun(value: Option<String>, offset: UtcOffset, date: &str) -> Result<Option<String>, Error> {
    value
        .map(|value| {
            let format =
                format_description::parse_borrowed::<2>("[year]-[month]-[day]T[hour]:[minute]")
                    .unwrap();
            let local =
                PrimitiveDateTime::parse(&value, &format).map_err(|_| Error::InvalidResponse)?;
            if local.date().to_string() != date {
                return Err(Error::InvalidResponse);
            }
            // Preserve the API's documented fixed offset for its daily aggregates.
            // This deliberately does not claim historical DST-aware day boundaries.
            local
                .assume_offset(offset)
                .format(&Rfc3339)
                .map_err(|_| Error::InvalidResponse)
        })
        .transpose()
}
fn decode(
    request: &PreviewRequest,
    status: u16,
    body: &[u8],
    now: OffsetDateTime,
) -> Result<Preview, Error> {
    if let Some(error) = response_error(status, body) {
        return Err(error);
    }
    if body.len() > BODY_LIMIT {
        return Err(Error::InvalidResponse);
    }
    let response: Response = serde_json::from_slice(body).map_err(|_| Error::InvalidResponse)?;
    if response.daily.time != [request.date.clone()]
        || response.utc_offset_seconds.unsigned_abs() > 14 * 3600
        || response.daily_units.get("time").map(String::as_str) != Some("iso8601")
    {
        return Err(Error::InvalidResponse);
    }
    let offset = UtcOffset::from_whole_seconds(response.utc_offset_seconds)
        .map_err(|_| Error::InvalidResponse)?;
    let d = response.daily;
    let mean = one(d.temperature_2m_mean)?;
    let min = one(d.temperature_2m_min)?;
    let max = one(d.temperature_2m_max)?;
    let precipitation = one(d.precipitation_sum)?;
    let wind = one(d.wind_speed_10m_max)?;
    let code = one(d.weather_code)?;
    for (field, unit, present) in [
        ("temperature_2m_mean", "°C", mean.is_some()),
        ("temperature_2m_min", "°C", min.is_some()),
        ("temperature_2m_max", "°C", max.is_some()),
        ("precipitation_sum", "mm", precipitation.is_some()),
        ("wind_speed_10m_max", "m/s", wind.is_some()),
        ("weather_code", "wmo code", code.is_some()),
    ] {
        if present && response.daily_units.get(field).map(String::as_str) != Some(unit) {
            return Err(Error::InvalidResponse);
        }
    }
    let temperature = |value: Option<f64>| {
        value
            .map(Celsius::try_from)
            .transpose()
            .map_err(|_| Error::InvalidResponse)
    };
    let fetched = FetchedWeather {
        valid_at: None,
        current_interval_seconds: None,
        temperature_c: None,
        wind_speed_ms: None,
        precipitation_mm: None,
        period: Period::Day {
            date: request.date.clone(),
        },
        location: Location {
            name: Some(request.place_name.clone()),
            coordinates: Some(Coordinates {
                latitude: request.latitude,
                longitude: request.longitude,
            }),
        },
        timezone: if response.timezone == "GMT" && response.utc_offset_seconds == 0 {
            "UTC".into()
        } else {
            response.timezone
        },
        source: Source {
            provider: Provider::OpenMeteo,
            product: "historical-weather".into(),
            model: Some("ERA5".into()),
            data_kind: DataKind::Reanalysis,
            sampled_coordinates: Some(Coordinates {
                latitude: response.latitude,
                longitude: response.longitude,
            }),
            attribution: "Weather data by Open-Meteo; ERA5 by Copernicus/ECMWF".into(),
            license: "CC BY 4.0".into(),
            license_url: Some("https://creativecommons.org/licenses/by/4.0/".into()),
        },
        retrieved_at: now
            .to_offset(UtcOffset::UTC)
            .format(&Rfc3339)
            .map_err(|_| Error::InvalidResponse)?,
        sunrise: sun(one(d.sunrise)?, offset, &request.date)?,
        sunset: sun(one(d.sunset)?, offset, &request.date)?,
        temperature_mean_c: temperature(mean)?,
        temperature_min_c: temperature(min)?,
        temperature_max_c: temperature(max)?,
        precipitation_sum_mm: precipitation,
        wind_speed_max_ms: wind,
        weather_code: code
            .map(WeatherCode::try_from)
            .transpose()
            .map_err(|_| Error::InvalidResponse)?,
        relative_humidity_percent: None,
        cloud_cover_percent: None,
        snow_depth_mm: None,
    };
    let json = serde_json::to_string(&fetched).map_err(|_| Error::InvalidResponse)?;
    super::validate_stored_parts("preview", None, Some(&json))
        .map_err(|_| Error::InvalidResponse)?;
    if mean.is_none()
        && min.is_none()
        && max.is_none()
        && precipitation.is_none()
        && wind.is_none()
        && code.is_none()
    {
        return Err(Error::NoData);
    }
    Ok(Preview { fetched })
}

fn preview_with(
    request: &PreviewRequest,
    now: OffsetDateTime,
    transport: impl FnOnce(reqwest::Url) -> Result<(u16, Vec<u8>), Error>,
) -> Result<Preview, Error> {
    validate_request(request, now)?;
    let (status, body) = transport(url(request)?)?;
    decode(request, status, &body, now)
}

#[cfg(test)]
fn preview_for_generation(
    state: &DatabaseState,
    generation: u32,
    request: &PreviewRequest,
    now: OffsetDateTime,
    transport: impl FnOnce(reqwest::Url) -> Result<(u16, Vec<u8>), Error>,
) -> Result<Snapshot<Preview>, Error> {
    drop(
        state
            .operation(Some(generation))
            .map_err(coordination_error)?,
    );
    let result = preview_with(request, now, transport)?;
    let operation = state
        .operation(Some(generation))
        .map_err(coordination_error)?;
    Ok(operation.snapshot(result))
}

#[cfg(test)]
fn coordination_error(error: String) -> Error {
    if error == crate::database_state::GENERATION_CONFLICT {
        Error::GenerationConflict
    } else {
        Error::DatabaseUnavailable
    }
}

#[cfg(test)]
mod tests;
