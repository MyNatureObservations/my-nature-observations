//! Weather contract v1. Pure validation/serialization: no storage, clock or network.
//! A parsed contract owns independent manual and provider snapshots, never journal text.
//! Canonical units: Celsius, metres/second and millimetres; missing values use null.
//! Round trips preserve values and user text, not JSON whitespace or key ordering.
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime, format_description, format_description::well_known::Rfc3339};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WeatherError {
    InvalidJson,
    UnsupportedVersion,
    InvalidEntryId,
    InvalidPeriod,
    InvalidLocation,
    InvalidTimezone,
    InvalidTemperature,
    InvalidTemperatureRange,
    InvalidWind,
    InvalidPrecipitation,
    InvalidWeatherCode,
    InvalidSource,
    InvalidRetrievedAt,
    InvalidPercentage,
    InvalidSunTime,
}
impl std::fmt::Display for WeatherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for WeatherError {}

// Display preferences deliberately have no representation in the stored contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TemperatureUnit {
    Celsius,
    Fahrenheit,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub(crate) struct Celsius(f64);
impl TryFrom<f64> for Celsius {
    type Error = WeatherError;
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value.is_finite() && value >= -273.15 {
            Ok(Self(value))
        } else {
            Err(WeatherError::InvalidTemperature)
        }
    }
}
impl From<Celsius> for f64 {
    fn from(value: Celsius) -> Self {
        value.0
    }
}
impl Celsius {
    pub(crate) fn from_display(value: f64, unit: TemperatureUnit) -> Result<Self, WeatherError> {
        if !value.is_finite() {
            return Err(WeatherError::InvalidTemperature);
        }
        Self::try_from(match unit {
            TemperatureUnit::Celsius => value,
            TemperatureUnit::Fahrenheit => {
                if value < -459.67 {
                    return Err(WeatherError::InvalidTemperature);
                }
                // Floating-point conversion can round the valid lower boundary
                // slightly below absolute zero; check the input before clamping.
                ((value - 32.0) / 1.8).max(-273.15)
            }
        })
    }
    pub(crate) fn display(self, unit: TemperatureUnit) -> Result<f64, WeatherError> {
        let value = match unit {
            TemperatureUnit::Celsius => self.0,
            TemperatureUnit::Fahrenheit => self.0 * 1.8 + 32.0,
        };
        if value.is_finite() {
            Ok(value)
        } else {
            Err(WeatherError::InvalidTemperature)
        }
    }
}

// Open-Meteo's WMO weather interpretation codes, not translated descriptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub(crate) struct WeatherCode(u8);
impl TryFrom<u8> for WeatherCode {
    type Error = WeatherError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0..=3
            | 45
            | 48
            | 51
            | 53
            | 55
            | 56
            | 57
            | 61
            | 63
            | 65
            | 66
            | 67
            | 71
            | 73
            | 75
            | 77
            | 80
            | 81
            | 82
            | 85
            | 86
            | 95
            | 96
            | 97
            | 99 => Ok(Self(value)),
            _ => Err(WeatherError::InvalidWeatherCode),
        }
    }
}
impl From<WeatherCode> for u8 {
    fn from(value: WeatherCode) -> Self {
        value.0
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Period {
    Day {
        date: String,
    },
    Hour {
        start: String,
        #[serde(rename = "endExclusive")]
        end_exclusive: String,
    },
}
impl Period {
    fn validate(&self) -> Result<(), WeatherError> {
        match self {
            Self::Day { date } => {
                let format = format_description::parse_borrowed::<2>("[year]-[month]-[day]")
                    .map_err(|_| WeatherError::InvalidPeriod)?;
                let parsed = Date::parse(date, &format).map_err(|_| WeatherError::InvalidPeriod)?;
                if date.len() != 10 || parsed.to_string() != *date {
                    return Err(WeatherError::InvalidPeriod);
                }
            }
            Self::Hour {
                start,
                end_exclusive,
            } => {
                let start = OffsetDateTime::parse(start, &Rfc3339)
                    .map_err(|_| WeatherError::InvalidPeriod)?;
                let end = OffsetDateTime::parse(end_exclusive, &Rfc3339)
                    .map_err(|_| WeatherError::InvalidPeriod)?;
                if end - start != time::Duration::hours(1) {
                    return Err(WeatherError::InvalidPeriod);
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Coordinates {
    latitude: f64,
    longitude: f64,
}
impl Coordinates {
    fn validate(&self) -> Result<(), WeatherError> {
        if self.latitude.is_finite()
            && self.longitude.is_finite()
            && (-90.0..=90.0).contains(&self.latitude)
            && (-180.0..=180.0).contains(&self.longitude)
        {
            Ok(())
        } else {
            Err(WeatherError::InvalidLocation)
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Location {
    name: Option<String>,
    coordinates: Option<Coordinates>,
}
impl Location {
    fn validate(&self) -> Result<(), WeatherError> {
        if let Some(name) = &self.name {
            if name.trim().is_empty() {
                return Err(WeatherError::InvalidLocation);
            }
        }
        if let Some(coordinates) = &self.coordinates {
            coordinates.validate()?;
        }
        if self.name.is_none() && self.coordinates.is_none() {
            return Err(WeatherError::InvalidLocation);
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManualWeather {
    period: Period,
    location: Option<Location>,
    timezone: Option<String>,
    description: Option<String>,
    temperature_c: Option<Celsius>,
    wind_speed_ms: Option<f64>,
    precipitation_mm: Option<f64>,
    weather_code: Option<WeatherCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    relative_humidity_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cloud_cover_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snow_depth_mm: Option<f64>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Provider {
    OpenMeteo,
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum DataKind {
    Reanalysis,
    ModelAnalysis,
    Forecast,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Source {
    provider: Provider,
    product: String,
    model: Option<String>,
    data_kind: DataKind,
    sampled_coordinates: Option<Coordinates>,
    attribution: String,
    license: String,
    license_url: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FetchedWeather {
    period: Period,
    location: Location,
    timezone: String,
    source: Source,
    retrieved_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    valid_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_interval_seconds: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature_c: Option<Celsius>,
    #[serde(skip_serializing_if = "Option::is_none")]
    wind_speed_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    precipitation_mm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sunrise: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sunset: Option<String>,
    temperature_mean_c: Option<Celsius>,
    temperature_min_c: Option<Celsius>,
    temperature_max_c: Option<Celsius>,
    precipitation_sum_mm: Option<f64>,
    wind_speed_max_ms: Option<f64>,
    weather_code: Option<WeatherCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    relative_humidity_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cloud_cover_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snow_depth_mm: Option<f64>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "WeatherWire", rename_all = "camelCase")]
pub(crate) struct WeatherContract {
    schema_version: u32,
    entry_id: String,
    manual: Option<ManualWeather>,
    fetched: Option<FetchedWeather>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WeatherWire {
    schema_version: u32,
    entry_id: String,
    manual: Option<ManualWeather>,
    fetched: Option<FetchedWeather>,
}

// This checks identifier syntax only. An adapter must resolve actual IANA zones
// and check historical offsets against the selected instant before saving data.
fn timezone_valid(value: &str) -> bool {
    value == "UTC"
        || (value.contains('/')
            && value.split('/').all(|part| {
                !part.is_empty()
                    && part != "."
                    && part != ".."
                    && part
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'+'))
            }))
}
fn nonnegative(value: Option<f64>, error: WeatherError) -> Result<(), WeatherError> {
    if value.is_some_and(|v| !v.is_finite() || v < 0.0) {
        Err(error)
    } else {
        Ok(())
    }
}
impl TryFrom<WeatherWire> for WeatherContract {
    type Error = WeatherError;
    fn try_from(wire: WeatherWire) -> Result<Self, Self::Error> {
        if wire.schema_version != 1 {
            return Err(WeatherError::UnsupportedVersion);
        }
        if wire.entry_id.trim().is_empty() {
            return Err(WeatherError::InvalidEntryId);
        }
        if let Some(manual) = &wire.manual {
            manual.period.validate()?;
            if let Some(location) = &manual.location {
                location.validate()?;
            }
            if manual
                .timezone
                .as_ref()
                .is_some_and(|tz| !timezone_valid(tz))
            {
                return Err(WeatherError::InvalidTimezone);
            }
            percentages(manual.relative_humidity_percent, manual.cloud_cover_percent)?;
            nonnegative(manual.snow_depth_mm, WeatherError::InvalidPrecipitation)?;
            nonnegative(manual.wind_speed_ms, WeatherError::InvalidWind)?;
            nonnegative(manual.precipitation_mm, WeatherError::InvalidPrecipitation)?;
        }
        if let Some(fetched) = &wire.fetched {
            fetched.period.validate()?;
            let current = fetched.source.product == "current-weather";
            if current {
                let valid = fetched
                    .valid_at
                    .as_deref()
                    .ok_or(WeatherError::InvalidPeriod)?;
                let valid = OffsetDateTime::parse(valid, &Rfc3339)
                    .map_err(|_| WeatherError::InvalidPeriod)?;
                if !matches!(&fetched.period, Period::Day {date} if *date == valid.date().to_string())
                    || fetched.source.data_kind != DataKind::Forecast
                    || !matches!(fetched.current_interval_seconds, Some(1..=3600))
                    || fetched.temperature_mean_c.is_some()
                    || fetched.temperature_min_c.is_some()
                    || fetched.temperature_max_c.is_some()
                    || fetched.wind_speed_max_ms.is_some()
                    || fetched.precipitation_sum_mm.is_some()
                    || fetched.sunrise.is_some()
                    || fetched.sunset.is_some()
                {
                    return Err(WeatherError::InvalidPeriod);
                }
            } else if fetched.valid_at.is_some()
                || fetched.current_interval_seconds.is_some()
                || fetched.temperature_c.is_some()
                || fetched.wind_speed_ms.is_some()
                || fetched.precipitation_mm.is_some()
            {
                return Err(WeatherError::InvalidPeriod);
            }
            nonnegative(fetched.wind_speed_ms, WeatherError::InvalidWind)?;
            nonnegative(fetched.precipitation_mm, WeatherError::InvalidPrecipitation)?;
            percentages(
                fetched.relative_humidity_percent,
                fetched.cloud_cover_percent,
            )?;
            nonnegative(fetched.snow_depth_mm, WeatherError::InvalidPrecipitation)?;
            for sun in [&fetched.sunrise, &fetched.sunset].into_iter().flatten() {
                OffsetDateTime::parse(sun, &Rfc3339).map_err(|_| WeatherError::InvalidSunTime)?;
            }
            if let (Some(rise), Some(set)) = (&fetched.sunrise, &fetched.sunset) {
                if OffsetDateTime::parse(rise, &Rfc3339).unwrap()
                    >= OffsetDateTime::parse(set, &Rfc3339).unwrap()
                {
                    return Err(WeatherError::InvalidSunTime);
                }
            }
            fetched.location.validate()?;
            if fetched.location.coordinates.is_none() {
                return Err(WeatherError::InvalidLocation);
            }
            if !timezone_valid(&fetched.timezone) {
                return Err(WeatherError::InvalidTimezone);
            }
            if let Some(coords) = &fetched.source.sampled_coordinates {
                coords.validate()?;
            }
            let source = &fetched.source;
            if [&source.product, &source.attribution, &source.license]
                .iter()
                .any(|v| v.trim().is_empty())
                || source.model.as_ref().is_some_and(|v| v.trim().is_empty())
                || source.license_url.as_ref().is_some_and(|v| {
                    !v.starts_with("https://") || v.len() <= 8 || v.chars().any(char::is_whitespace)
                })
            {
                return Err(WeatherError::InvalidSource);
            }
            let retrieved = OffsetDateTime::parse(&fetched.retrieved_at, &Rfc3339)
                .map_err(|_| WeatherError::InvalidRetrievedAt)?;
            if retrieved.offset() != time::UtcOffset::UTC {
                return Err(WeatherError::InvalidRetrievedAt);
            }
            nonnegative(fetched.wind_speed_max_ms, WeatherError::InvalidWind)?;
            nonnegative(
                fetched.precipitation_sum_mm,
                WeatherError::InvalidPrecipitation,
            )?;
            if let (Some(min), Some(max)) = (fetched.temperature_min_c, fetched.temperature_max_c) {
                if min.0 > max.0 {
                    return Err(WeatherError::InvalidTemperatureRange);
                }
            }
            if let Some(mean) = fetched.temperature_mean_c {
                if fetched.temperature_min_c.is_some_and(|min| mean.0 < min.0)
                    || fetched.temperature_max_c.is_some_and(|max| mean.0 > max.0)
                {
                    return Err(WeatherError::InvalidTemperatureRange);
                }
            }
        }
        Ok(Self {
            schema_version: wire.schema_version,
            entry_id: wire.entry_id,
            manual: wire.manual,
            fetched: wire.fetched,
        })
    }
}
impl WeatherContract {
    pub(crate) fn from_json(json: &str) -> Result<Self, WeatherError> {
        let wire: WeatherWire =
            serde_json::from_str(json).map_err(|_| WeatherError::InvalidJson)?;
        // Preserve domain errors here; direct serde consumers use the same gate.
        wire.try_into()
    }
    pub(crate) fn to_json(&self) -> Result<String, WeatherError> {
        serde_json::to_string(self).map_err(|_| WeatherError::InvalidJson)
    }
}

#[cfg(test)]
mod tests;

// Isolated database adapter: parse the original part strings directly so duplicate
// fields and unknown nodes are rejected by the same typed contract, not Value.
pub(crate) fn validate_stored_parts(
    entry_id: &str,
    manual: Option<&str>,
    fetched: Option<&str>,
) -> Result<(), WeatherError> {
    let wire = WeatherWire {
        schema_version: 1,
        entry_id: entry_id.to_owned(),
        manual: manual
            .map(serde_json::from_str::<ManualWeather>)
            .transpose()
            .map_err(|_| WeatherError::InvalidJson)?,
        fetched: fetched
            .map(serde_json::from_str::<FetchedWeather>)
            .transpose()
            .map_err(|_| WeatherError::InvalidJson)?,
    };
    WeatherContract::try_from(wire).map(|_| ())
}

fn percentages(humidity: Option<f64>, clouds: Option<f64>) -> Result<(), WeatherError> {
    if [humidity, clouds]
        .into_iter()
        .flatten()
        .any(|v| !v.is_finite() || !(0.0..=100.0).contains(&v))
    {
        return Err(WeatherError::InvalidPercentage);
    }
    Ok(())
}
#[cfg(all(test, target_os = "macos"))]
mod stage1_tests;
pub(crate) mod storage;

pub(crate) mod historical;
pub(crate) mod view;
