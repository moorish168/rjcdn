//! MaxMind GeoLite2-Country lookups for the control-plane scheduler / DNS line match.
//!
//! Same MMDB family as the edge WAF (`GEOIP_COUNTRY_DB`). Country ISO codes and
//! continent codes (NA/EU/AS/…) come from one file; province/city/ISP are empty
//! because Country does not carry them.

use maxminddb::Reader;
use serde::Deserialize;
use std::net::IpAddr;
use std::sync::Arc;

use super::GeoInfo;

#[derive(Debug, Deserialize)]
struct CountryRecord {
    #[serde(default)]
    continent: Option<Continent>,
    #[serde(default)]
    country: Option<IsoCountry>,
    #[serde(default)]
    registered_country: Option<IsoCountry>,
}

#[derive(Debug, Deserialize)]
struct Continent {
    #[serde(default)]
    code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IsoCountry {
    #[serde(default)]
    iso_code: Option<String>,
}

pub struct MaxMindCountryDb {
    reader: Reader<Vec<u8>>,
}

impl MaxMindCountryDb {
    pub fn open(path: &str) -> Result<Self, String> {
        let reader = Reader::open_readfile(path).map_err(|e| e.to_string())?;
        Ok(Self { reader })
    }

    pub fn lookup(&self, ip: IpAddr) -> Option<GeoInfo> {
        let record: CountryRecord = self.reader.lookup(ip).ok()?;
        let country = record
            .country
            .and_then(|c| c.iso_code)
            .or_else(|| record.registered_country.and_then(|c| c.iso_code))
            .map(|c| c.trim().to_ascii_uppercase())
            .filter(|c| c.len() == 2)?;
        let continent = record
            .continent
            .and_then(|c| c.code)
            .map(|c| c.trim().to_ascii_uppercase())
            .filter(|c| !c.is_empty())
            .unwrap_or_default();
        Some(GeoInfo {
            continent,
            country,
            province: String::new(),
            city: String::new(),
            isp: String::new(),
        })
    }
}

pub fn is_mmdb_path(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("mmdb"))
        .unwrap_or(false)
}

pub type SharedMaxMind = Arc<MaxMindCountryDb>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use std::path::PathBuf;

    fn sample_db() -> Option<MaxMindCountryDb> {
        let candidates = [
            PathBuf::from("data/maxmind/GeoLite2-Country.mmdb"),
            PathBuf::from("../data/maxmind/GeoLite2-Country.mmdb"),
            PathBuf::from("../../data/maxmind/GeoLite2-Country.mmdb"),
        ];
        for path in candidates {
            if path.is_file() {
                return MaxMindCountryDb::open(path.to_str().unwrap()).ok();
            }
        }
        None
    }

    #[test]
    fn known_public_resolvers_map_to_expected_countries() {
        let Some(db) = sample_db() else {
            eprintln!("skip: GeoLite2-Country.mmdb not present");
            return;
        };
        let us = db
            .lookup(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)))
            .expect("8.8.8.8 in MaxMind");
        assert_eq!(us.country, "US", "{us:?}");
        assert_eq!(us.continent, "NA", "{us:?}");

        let tw = db
            .lookup(IpAddr::V4(Ipv4Addr::new(1, 34, 1, 1)))
            .expect("1.34.1.1 in MaxMind");
        assert_eq!(tw.country, "TW", "{tw:?}");
    }
}
