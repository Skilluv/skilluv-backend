use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Country {
    pub iso2: String,
    pub iso3: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct City {
    pub name: String,
    pub country: String,
    pub population: i64,
    #[serde(skip)]
    pub asciiname_lower: String,
    #[serde(skip)]
    pub name_lower: String,
}

pub struct GeoService {
    countries: Vec<Country>,
    iso3_to_iso2: HashMap<String, String>,
    cities_by_country: HashMap<String, Vec<City>>,
}

impl GeoService {
    pub fn load(data_dir: &Path) -> std::io::Result<Self> {
        let (countries, iso3_to_iso2) = Self::load_countries(data_dir)?;
        let cities_by_country = Self::load_cities(data_dir)?;
        Ok(Self {
            countries,
            iso3_to_iso2,
            cities_by_country,
        })
    }

    fn load_countries(data_dir: &Path) -> std::io::Result<(Vec<Country>, HashMap<String, String>)> {
        let file = File::open(data_dir.join("countryInfo.txt"))?;
        let mut countries = Vec::new();
        let mut iso3_to_iso2 = HashMap::new();

        for line in BufReader::new(file).lines() {
            let line = line?;
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let cols: Vec<&str> = line.split('\t').collect();
            if cols.len() < 5 {
                continue;
            }
            let iso2 = cols[0].trim().to_string();
            let iso3 = cols[1].trim().to_string();
            let name = cols[4].trim().to_string();
            if iso2.is_empty() || iso3.is_empty() || name.is_empty() {
                continue;
            }
            iso3_to_iso2.insert(iso3.clone(), iso2.clone());
            countries.push(Country { iso2, iso3, name });
        }
        countries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok((countries, iso3_to_iso2))
    }

    fn load_cities(data_dir: &Path) -> std::io::Result<HashMap<String, Vec<City>>> {
        let file = File::open(data_dir.join("cities1000.txt"))?;
        let mut cities_by_country: HashMap<String, Vec<City>> = HashMap::new();

        for line in BufReader::new(file).lines() {
            let line = line?;
            let cols: Vec<&str> = line.split('\t').collect();
            if cols.len() < 15 {
                continue;
            }
            let name = cols[1].to_string();
            let asciiname = cols[2].to_string();
            let country = cols[8].trim().to_string();
            let population: i64 = cols[14].parse().unwrap_or(0);
            if country.is_empty() || name.is_empty() {
                continue;
            }
            cities_by_country
                .entry(country.clone())
                .or_default()
                .push(City {
                    asciiname_lower: asciiname.to_lowercase(),
                    name_lower: name.to_lowercase(),
                    name,
                    country,
                    population,
                });
        }
        for cities in cities_by_country.values_mut() {
            cities.sort_by(|a, b| b.population.cmp(&a.population));
        }
        Ok(cities_by_country)
    }

    pub fn countries(&self) -> &[Country] {
        &self.countries
    }

    pub fn total_cities(&self) -> usize {
        self.cities_by_country.values().map(|v| v.len()).sum()
    }

    pub fn search_cities(
        &self,
        country_code: &str,
        query: Option<&str>,
        limit: usize,
    ) -> Vec<&City> {
        let upper = country_code.trim().to_uppercase();
        let iso2 = if upper.len() == 3 {
            match self.iso3_to_iso2.get(&upper) {
                Some(v) => v.clone(),
                None => return Vec::new(),
            }
        } else {
            upper
        };
        let Some(list) = self.cities_by_country.get(&iso2) else {
            return Vec::new();
        };
        match query.map(str::trim).filter(|q| !q.is_empty()) {
            Some(q) => {
                let needle = q.to_lowercase();
                list.iter()
                    .filter(|c| {
                        c.asciiname_lower.starts_with(&needle) || c.name_lower.starts_with(&needle)
                    })
                    .take(limit)
                    .collect()
            }
            None => list.iter().take(limit).collect(),
        }
    }
}
