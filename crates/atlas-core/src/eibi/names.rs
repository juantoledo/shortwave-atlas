//! Display names and groupings Atlas adds to EiBi's codes: target areas, world regions,
//! broadcast bands, and English/Spanish names for the common languages.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::country::{country, Region};

/// A target-area code: English and Spanish names and its world region.
pub struct Target {
    pub code: &'static str,
    pub en: &'static str,
    pub es: &'static str,
    pub region: Region,
}

macro_rules! targets {
    ($($code:literal $region:ident $en:literal $es:literal;)*) => {
        &[$(Target { code: $code, en: $en, es: $es, region: Region::$region }),*]
    };
}

/// EiBi's target-area codes (README D.III, with the C/E/N/S/W prefixes spelled out).
/// Country codes are also used as targets; those come from `country`.
pub static TARGETS: &[Target] = targets! {
    "Af" Af "Africa" "África";
    "Am" Am "Americas" "América";
    "As" As "Asia" "Asia";
    "CAf" Af "Central Africa" "África central";
    "CAm" Am "Central America" "Centroamérica";
    "CAs" As "Central Asia" "Asia central";
    "CEu" Eu "Central Europe" "Europa central";
    "CIS" Eu "Former Soviet Union" "Antigua Unión Soviética";
    "CNA" Am "Central North America" "Centro de Norteamérica";
    "COc" Oc "Central Oceania" "Oceanía central";
    "Car" Am "Caribbean" "Caribe";
    "Cau" Me "Caucasus" "Cáucaso";
    "EAf" Af "East Africa" "África oriental";
    "EAs" As "East Asia" "Asia oriental";
    "EEu" Eu "Eastern Europe" "Europa oriental";
    "EIn" As "Eastern India" "India oriental";
    "ENA" Am "Eastern North America" "Este de Norteamérica";
    "ENE" Other "East-northeast" "Estenoreste";
    "EOc" Oc "Eastern Oceania" "Oceanía oriental";
    "ESE" Other "East-southeast" "Estesureste";
    "Eu" Eu "Europe" "Europa";
    "FE" As "Far East" "Lejano Oriente";
    "Glo" Other "Worldwide" "Mundial";
    "In" As "Indian subcontinent" "Subcontinente indio";
    "LAm" Am "Latin America" "Latinoamérica";
    "ME" Me "Middle East" "Oriente Medio";
    "NAO" Other "North Atlantic Ocean" "Atlántico Norte";
    "NAf" Af "North Africa" "Norte de África";
    "NAm" Am "North America" "Norteamérica";
    "NAs" As "North Asia" "Norte de Asia";
    "NE" Other "Northeast" "Noreste";
    "NEu" Eu "Northern Europe" "Europa del norte";
    "NIn" As "Northern India" "Norte de la India";
    "NNE" Other "North-northeast" "Nornoreste";
    "NNW" Other "North-northwest" "Nornoroeste";
    "NOc" Oc "Northern Oceania" "Oceanía septentrional";
    "NW" Other "Northwest" "Noroeste";
    "Oc" Oc "Oceania" "Oceanía";
    "SAO" Other "South Atlantic Ocean" "Atlántico Sur";
    "SAf" Af "Southern Africa" "África austral";
    "SAm" Am "South America" "Sudamérica";
    "SAs" As "South Asia" "Asia meridional";
    "SE" Other "Southeast" "Sureste";
    "SEA" As "Southeast Asia" "Sudeste asiático";
    "SEE" Eu "Southeastern Europe" "Sudeste de Europa";
    "SEu" Eu "Southern Europe" "Europa del sur";
    "SIn" As "Southern India" "Sur de la India";
    "SOc" Oc "Southern Oceania" "Oceanía meridional";
    "SSE" Other "South-southeast" "Sursureste";
    "SSW" Other "South-southwest" "Sursuroeste";
    "SW" Other "Southwest" "Suroeste";
    "Sib" As "Siberia" "Siberia";
    "Tas" Oc "Tasmania" "Tasmania";
    "Tib" As "Tibet" "Tíbet";
    "WAf" Af "West Africa" "África occidental";
    "WAs" Me "Western Asia" "Asia occidental";
    "WEu" Eu "Western Europe" "Europa occidental";
    "WIO" Other "Western Indian Ocean" "Océano Índico occidental";
    "WIn" As "Western India" "India occidental";
    "WNA" Am "Western North America" "Oeste de Norteamérica";
    "WNW" Other "West-northwest" "Oesnoroeste";
    "WOc" Oc "Western Oceania" "Oceanía occidental";
    "WSW" Other "West-southwest" "Oessuroeste";
};

pub fn target(code: &str) -> Option<&'static Target> {
    TARGETS.iter().find(|t| t.code == code)
}

/// The world region of a target code (an area or a country); `Other` if unknown.
pub fn target_region(code: &str) -> Region {
    target(code).map(|t| t.region).or_else(|| country(code).map(|c| c.region)).unwrap_or(Region::Other)
}

/// A shortwave broadcast band.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Band {
    /// `49m`
    pub id: String,
    pub lo_khz: u32,
    pub hi_khz: u32,
}

/// Broadcast bands with the edges stations actually use (a little wider than the ITU
/// allocations). Frequencies outside all of them are "out of band".
pub const BANDS: [(&str, u32, u32); 14] = [
    ("120m", 2300, 2500),
    ("90m", 3200, 3400),
    ("75m", 3900, 4050),
    ("60m", 4750, 5100),
    ("49m", 5800, 6300),
    ("41m", 7200, 7600),
    ("31m", 9300, 10_000),
    ("25m", 11_500, 12_200),
    ("22m", 13_500, 13_900),
    ("19m", 15_000, 15_900),
    ("16m", 17_400, 17_950),
    ("15m", 18_850, 19_100),
    ("13m", 21_400, 21_900),
    ("11m", 25_600, 26_100),
];

/// Band id for "outside every broadcast band".
pub const OUT_OF_BAND: &str = "oob";

/// The band id for a frequency in Hz (`OUT_OF_BAND` if none).
pub fn band_of(freq_hz: u32) -> &'static str {
    let khz = freq_hz / 1000;
    BANDS.iter().find(|b| khz >= b.1 && khz <= b.2).map_or(OUT_OF_BAND, |b| b.0)
}

pub fn bands() -> Vec<Band> {
    BANDS.iter().map(|&(id, lo, hi)| Band { id: id.into(), lo_khz: lo, hi_khz: hi }).collect()
}

/// English name override (when the README's is awkward) and Spanish name of a language.
pub struct LangName {
    pub code: &'static str,
    pub en: Option<&'static str>,
    pub es: &'static str,
}

macro_rules! langs {
    ($($code:literal $en:tt $es:literal;)*) => {
        &[$(LangName { code: $code, en: langs!(@en $en), es: $es }),*]
    };
    (@en -) => { None };
    (@en $e:literal) => { Some($e) };
}

/// The languages heard most often; the rest keep the README's English name (and the UI
/// may localise them through their ISO 639-3 code).
pub static LANGS: &[LangName] = langs! {
    "-MX" - "Música";
    "A" - "Árabe";
    "AF" - "Afrikáans";
    "AFG" - "Pastún y darí";
    "AH" - "Amhárico";
    "AL" - "Albanés";
    "AM" - "Amoy";
    "AR" - "Armenio";
    "AZ" "Azerbaijani" "Azerí";
    "BE" "Bengali" "Bengalí";
    "BM" "Bambara" "Bambara";
    "BR" "Burmese" "Birmano";
    "BU" - "Búlgaro";
    "BY" "Belarusian" "Bielorruso";
    "C" - "Chino";
    "CA" "Cantonese" "Cantonés";
    "CHE" - "Checheno";
    "CR" "Haitian Creole" "Criollo haitiano";
    "CZ" - "Checo";
    "D" - "Alemán";
    "DA" - "Danés";
    "DR" "Dari" "Darí";
    "E" - "Inglés";
    "EO" - "Esperanto";
    "F" - "Francés";
    "FI" - "Finés";
    "FS" "Persian (Farsi)" "Persa (farsi)";
    "FU" "Fula" "Fula";
    "GE" - "Georgiano";
    "GR" - "Griego";
    "HA" "Hausa" "Hausa";
    "HB" - "Hebreo";
    "HI" - "Hindi";
    "HK" - "Hakka";
    "HR" "Croatian" "Croata";
    "HU" - "Húngaro";
    "I" - "Italiano";
    "IG" "Igbo" "Igbo";
    "IN" "Indonesian" "Indonesio";
    "J" - "Japonés";
    "JV" - "Javanés";
    "K" - "Coreano";
    "KG" "Kyrgyz" "Kirguís";
    "KH" - "Jemer";
    "KU" - "Kurdo";
    "KZ" - "Kazajo";
    "L" - "Latín";
    "LAO" - "Lao";
    "M" - "Mandarín";
    "MAL" - "Malayalam";
    "MAR" - "Maratí";
    "ML" "Malay" "Malayo";
    "MO" - "Mongol";
    "MSY" - "Malgache";
    "NE" "Nepali" "Nepalí";
    "NL" - "Neerlandés";
    "NO" - "Noruego";
    "OO" - "Oromo";
    "P" - "Portugués";
    "PJ" - "Panyabí";
    "PO" - "Polaco";
    "PS" "Pashto" "Pastún";
    "Q" - "Quechua";
    "R" - "Ruso";
    "RO" - "Rumano";
    "S" "Spanish" "Español";
    "SEF" "Ladino (Judeo-Spanish)" "Ladino (judeoespañol)";
    "SI" "Sinhala" "Cingalés";
    "SK" - "Eslovaco";
    "SM" - "Samoano";
    "SO" - "Somalí";
    "SR" - "Serbio";
    "SUD" - "Árabe sudanés";
    "SWA" "Swahili" "Suajili";
    "T" - "Tailandés";
    "TAG" - "Tagalo";
    "TAM" - "Tamil";
    "TB" "Tibetan" "Tibetano";
    "TEL" - "Telugu";
    "TIG" "Tigrinya" "Tigriña";
    "TJ" - "Tayiko";
    "TK" - "Turcomano";
    "TO" - "Tongano";
    "TT" - "Tártaro";
    "TU" - "Turco";
    "UI" "Uyghur" "Uigur";
    "UK" - "Ucraniano";
    "UR" - "Urdu";
    "UZ" - "Uzbeko";
    "VN" - "Vietnamita";
    "YO" - "Yoruba";
};

pub fn lang_name(code: &str) -> Option<&'static LangName> {
    LANGS.iter().find(|l| l.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_regions() {
        assert_eq!(target_region("CAm"), Region::Am);
        assert_eq!(target_region("FE"), Region::As);
        assert_eq!(target_region("CHN"), Region::As);
        assert_eq!(target_region("E"), Region::Eu); // Spain, not "east"
        assert_eq!(target_region("NNE"), Region::Other);
        assert_eq!(target_region("???"), Region::Other);
    }

    #[test]
    fn tables_have_unique_codes() {
        let mut t: Vec<_> = TARGETS.iter().map(|t| t.code).collect();
        t.sort_unstable();
        t.dedup();
        assert_eq!(t.len(), TARGETS.len());
        let mut l: Vec<_> = LANGS.iter().map(|l| l.code).collect();
        l.sort_unstable();
        l.dedup();
        assert_eq!(l.len(), LANGS.len());
    }

    #[test]
    fn bands_by_frequency() {
        assert_eq!(band_of(6_070_000), "49m");
        assert_eq!(band_of(9_410_000), "31m");
        assert_eq!(band_of(5_025_000), "60m");
        assert_eq!(band_of(26_000_000), "11m");
        assert_eq!(band_of(8_000_000), OUT_OF_BAND);
        assert!(BANDS.windows(2).all(|w| w[0].2 < w[1].1));
    }
}
