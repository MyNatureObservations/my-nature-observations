//! Date routing uses the API's fixed local offset, without a timezone database.
use super::*;
use std::collections::BTreeMap;
const FORECAST: &str = "https://api.open-meteo.com/v1/forecast";
const CURRENT: &str =
    "temperature_2m,relative_humidity_2m,precipitation,weather_code,wind_speed_10m";

#[derive(Debug, PartialEq)]
enum Route {
    Current,
    Daily,
    Era5,
}
fn route(date: Date, now: OffsetDateTime, offset: UtcOffset) -> Result<Route, Error> {
    let age = (now.to_offset(offset).date() - date).whole_days();
    match age {
        0 => Ok(Route::Current),
        1..=6 => Ok(Route::Daily),
        7.. => Ok(Route::Era5),
        _ => Err(Error::DateUnavailable),
    }
}
#[derive(Deserialize)]
struct Metadata {
    latitude: f64,
    longitude: f64,
    timezone: String,
    utc_offset_seconds: i32,
    current: Option<Current>,
    current_units: Option<BTreeMap<String, String>>,
}
#[derive(Deserialize)]
struct Current {
    time: String,
    interval: u32,
    temperature_2m: Option<f64>,
    relative_humidity_2m: Option<f64>,
    precipitation: Option<f64>,
    weather_code: Option<u8>,
    wind_speed_10m: Option<f64>,
}
fn forecast_url(request: &PreviewRequest, current: bool) -> Result<reqwest::Url, Error> {
    let mut result = url(request)?;
    result
        .set_host(Some("api.open-meteo.com"))
        .map_err(|_| Error::Network)?;
    result.set_path("/v1/forecast");
    let pairs: Vec<_> = result
        .query_pairs()
        .filter(|(k, _)| !["models", "daily", "start_date", "end_date"].contains(&k.as_ref()))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    result.set_query(None);
    result
        .query_pairs_mut()
        .extend_pairs(pairs)
        .append_pair("models", "ecmwf_ifs");
    if current {
        result
            .query_pairs_mut()
            .append_pair("forecast_days", "0")
            .append_pair("current", CURRENT);
    } else {
        result
            .query_pairs_mut()
            .append_pair("start_date", &request.date)
            .append_pair("end_date", &request.date)
            .append_pair("daily", DAILY);
    }
    debug_assert!(result.as_str().starts_with(FORECAST));
    Ok(result)
}
fn metadata(status: u16, body: &[u8]) -> Result<Metadata, Error> {
    if let Some(error) = response_error(status, body) {
        return Err(error);
    }
    if body.len() > BODY_LIMIT {
        return Err(Error::InvalidResponse);
    }
    let mut result: Metadata = serde_json::from_slice(body).map_err(|_| Error::InvalidResponse)?;
    if result.timezone == "GMT" && result.utc_offset_seconds == 0 {
        result.timezone = "UTC".into();
    }
    Coordinates {
        latitude: result.latitude,
        longitude: result.longitude,
    }
    .validate()
    .map_err(|_| Error::InvalidResponse)?;
    if result.utc_offset_seconds.unsigned_abs() > 14 * 3600
        || !super::super::timezone_valid(&result.timezone)
    {
        return Err(Error::InvalidResponse);
    }
    Ok(result)
}
fn source(meta: &Metadata, product: &str) -> Source {
    Source {
        provider: Provider::OpenMeteo,
        product: product.into(),
        model: Some("ECMWF IFS HRES (ecmwf_ifs)".into()),
        data_kind: DataKind::Forecast,
        sampled_coordinates: Some(Coordinates {
            latitude: meta.latitude,
            longitude: meta.longitude,
        }),
        attribution: "Weather data by Open-Meteo; model data by ECMWF".into(),
        license: "CC BY 4.0".into(),
        license_url: Some("https://creativecommons.org/licenses/by/4.0/".into()),
    }
}
fn current(
    request: &PreviewRequest,
    mut meta: Metadata,
    now: OffsetDateTime,
) -> Result<Preview, Error> {
    let source = source(&meta, "current-weather");
    let data = meta.current.take().ok_or(Error::NoData)?;
    let units = meta.current_units.take().ok_or(Error::InvalidResponse)?;
    for (key, unit, present) in [
        ("time", "iso8601", true),
        ("interval", "seconds", true),
        ("temperature_2m", "°C", data.temperature_2m.is_some()),
        (
            "relative_humidity_2m",
            "%",
            data.relative_humidity_2m.is_some(),
        ),
        ("precipitation", "mm", data.precipitation.is_some()),
        ("weather_code", "wmo code", data.weather_code.is_some()),
        ("wind_speed_10m", "m/s", data.wind_speed_10m.is_some()),
    ] {
        if present && units.get(key).map(String::as_str) != Some(unit) {
            return Err(Error::InvalidResponse);
        }
    }
    if data.interval == 0 || data.interval > 3600 {
        return Err(Error::InvalidResponse);
    }
    let format =
        format_description::parse_borrowed::<2>("[year]-[month]-[day]T[hour]:[minute]").unwrap();
    let valid = PrimitiveDateTime::parse(&data.time, &format)
        .map_err(|_| Error::InvalidResponse)?
        .assume_offset(
            UtcOffset::from_whole_seconds(meta.utc_offset_seconds)
                .map_err(|_| Error::InvalidResponse)?,
        );
    // A current model slot can be nearby, but never another journal day or a
    // stale/future forecast masquerading as current conditions.
    if valid.date().to_string() != request.date
        || (valid - now).whole_seconds().unsigned_abs() > 3600
        || valid > now + time::Duration::seconds(i64::from(data.interval))
    {
        return Err(Error::NoData);
    }
    if data.temperature_2m.is_none()
        && data.wind_speed_10m.is_none()
        && data.precipitation.is_none()
        && data.weather_code.is_none()
        && data.relative_humidity_2m.is_none()
    {
        return Err(Error::NoData);
    }
    let fetched = FetchedWeather {
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
        timezone: if meta.timezone == "GMT" {
            "UTC".into()
        } else {
            meta.timezone
        },
        source,
        retrieved_at: now
            .to_offset(UtcOffset::UTC)
            .format(&Rfc3339)
            .map_err(|_| Error::InvalidResponse)?,
        valid_at: Some(valid.format(&Rfc3339).map_err(|_| Error::InvalidResponse)?),
        current_interval_seconds: Some(data.interval),
        temperature_c: data
            .temperature_2m
            .map(Celsius::try_from)
            .transpose()
            .map_err(|_| Error::InvalidResponse)?,
        wind_speed_ms: data.wind_speed_10m,
        precipitation_mm: data.precipitation,
        temperature_mean_c: None,
        temperature_min_c: None,
        temperature_max_c: None,
        precipitation_sum_mm: None,
        wind_speed_max_ms: None,
        weather_code: data
            .weather_code
            .map(WeatherCode::try_from)
            .transpose()
            .map_err(|_| Error::InvalidResponse)?,
        relative_humidity_percent: data.relative_humidity_2m,
        cloud_cover_percent: None,
        snow_depth_mm: None,
        sunrise: None,
        sunset: None,
    };
    super::super::validate_stored_parts(
        "preview",
        None,
        Some(&serde_json::to_string(&fetched).map_err(|_| Error::InvalidResponse)?),
    )
    .map_err(|_| Error::InvalidResponse)?;
    Ok(Preview { fetched })
}

pub(super) fn preview(
    request: &PreviewRequest,
    now: OffsetDateTime,
    mut transport: impl FnMut(reqwest::Url) -> Result<(u16, Vec<u8>), Error>,
) -> Result<Preview, Error> {
    Coordinates {
        latitude: request.latitude,
        longitude: request.longitude,
    }
    .validate()
    .map_err(|_| Error::InvalidCoordinates)?;
    if request.place_name.trim().is_empty() || request.place_name.len() > 512 {
        return Err(Error::InvalidPlace);
    }
    let format = format_description::parse_borrowed::<2>("[year]-[month]-[day]").unwrap();
    let date = Date::parse(&request.date, &format).map_err(|_| Error::InvalidDate)?;
    if date.to_string() != request.date || request.date.len() != 10 {
        return Err(Error::InvalidDate);
    }
    if request.date.as_str() < "1940-01-01" || date > now.date() + time::Duration::days(1) {
        return Err(Error::DateUnavailable);
    }
    // At least eight UTC days old is at least seven local days everywhere.
    // Preserve the single-request ERA5 path for older journal entries.
    if date <= now.to_offset(UtcOffset::UTC).date() - time::Duration::days(8) {
        return preview_with(request, now, transport);
    }
    let (status, body) = transport(forecast_url(request, true)?)?;
    let meta = metadata(status, &body)?;
    let offset = UtcOffset::from_whole_seconds(meta.utc_offset_seconds)
        .map_err(|_| Error::InvalidResponse)?;
    match route(date, now, offset)? {
        Route::Current => current(request, meta, now),
        Route::Era5 => preview_with(request, now, transport),
        Route::Daily => {
            let (status, body) = transport(forecast_url(request, false)?)?;
            let mut result = decode(request, status, &body, now)?;
            let timezone = if meta.timezone == "GMT" {
                "UTC"
            } else {
                &meta.timezone
            };
            if result.fetched.timezone != timezone {
                return Err(Error::InvalidResponse);
            }
            let sampled = result.fetched.source.sampled_coordinates.take();
            result.fetched.source = source(&meta, "forecast-daily");
            result.fetched.source.sampled_coordinates = sampled;
            Ok(result)
        }
    }
}

#[cfg(test)]
mod tests;
