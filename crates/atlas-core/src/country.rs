//! EiBi (ITU) country codes: flag, world region and display names.
//!
//! EiBi uses ITU-style codes (`G` = United Kingdom, `D` = Germany, `HOL` = Netherlands) plus
//! some of its own (`CLA` = clandestine, `XUU` = unidentified). `flag` is a key of the
//! `flag-icons` package (ISO 3166-1 alpha-2 in lowercase, or one of its extras such as
//! `sh-ac`, `ic`, `xk`, `un`, `xx`); `None` gets a generic icon in the UI.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// World regions for the target-area filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Region {
    /// The Americas
    Am,
    Eu,
    Af,
    /// Middle East and Caucasus
    Me,
    As,
    Oc,
    /// Oceans, Antarctica, worldwide, compass directions
    Other,
}

impl Region {
    pub const ALL: [Region; 7] =
        [Region::Am, Region::Eu, Region::Af, Region::Me, Region::As, Region::Oc, Region::Other];

    pub fn id(self) -> &'static str {
        match self {
            Region::Am => "am",
            Region::Eu => "eu",
            Region::Af => "af",
            Region::Me => "me",
            Region::As => "as",
            Region::Oc => "oc",
            Region::Other => "other",
        }
    }
}

pub struct Country {
    pub itu: &'static str,
    pub flag: Option<&'static str>,
    pub region: Region,
    pub en: &'static str,
    pub es: &'static str,
}

use Region::{Af, Am, As, Eu, Me, Oc, Other};

macro_rules! countries {
    ($($itu:literal $flag:tt $region:ident $en:literal $es:literal;)*) => {
        &[$(Country { itu: $itu, flag: countries!(@flag $flag), region: $region, en: $en, es: $es }),*]
    };
    (@flag -) => { None };
    (@flag $f:literal) => { Some($f) };
}

/// Every country code in EiBi's README.TXT (section D.II), sorted by code.
pub static COUNTRIES: &[Country] = countries! {
    "ABW" "aw" Am "Aruba" "Aruba";
    "AFG" "af" As "Afghanistan" "Afganistán";
    "AFS" "za" Af "South Africa" "Sudáfrica";
    "AGL" "ao" Af "Angola" "Angola";
    "AIA" "ai" Am "Anguilla" "Anguila";
    "ALB" "al" Eu "Albania" "Albania";
    "ALG" "dz" Af "Algeria" "Argelia";
    "ALS" "us" Am "Alaska" "Alaska";
    "AMS" "tf" Other "Saint Paul and Amsterdam Islands" "Islas San Pablo y Ámsterdam";
    "AND" "ad" Eu "Andorra" "Andorra";
    "AOE" "eh" Af "Western Sahara" "Sahara Occidental";
    "ARG" "ar" Am "Argentina" "Argentina";
    "ARM" "am" Me "Armenia" "Armenia";
    "ARS" "sa" Me "Saudi Arabia" "Arabia Saudita";
    "ASC" "sh-ac" Af "Ascension Island" "Isla Ascensión";
    "ATA" "aq" Other "Antarctica" "Antártida";
    "ATG" "ag" Am "Antigua and Barbuda" "Antigua y Barbuda";
    "ATN" - Am "Netherlands Antilles" "Antillas Neerlandesas";
    "AUS" "au" Oc "Australia" "Australia";
    "AUT" "at" Eu "Austria" "Austria";
    "AZE" "az" Me "Azerbaijan" "Azerbaiyán";
    "AZR" "pt" Eu "Azores" "Azores";
    "B" "br" Am "Brazil" "Brasil";
    "BAH" "bs" Am "Bahamas" "Bahamas";
    "BDI" "bi" Af "Burundi" "Burundi";
    "BEL" "be" Eu "Belgium" "Bélgica";
    "BEN" "bj" Af "Benin" "Benín";
    "BER" "bm" Am "Bermuda" "Bermudas";
    "BES" "bq" Am "Caribbean Netherlands" "Caribe Neerlandés";
    "BFA" "bf" Af "Burkina Faso" "Burkina Faso";
    "BGD" "bd" As "Bangladesh" "Bangladés";
    "BHR" "bh" Me "Bahrain" "Baréin";
    "BIH" "ba" Eu "Bosnia and Herzegovina" "Bosnia y Herzegovina";
    "BIO" "io" Other "Diego Garcia (Chagos Islands)" "Diego García (islas Chagos)";
    "BLM" "bl" Am "Saint Barthélemy" "San Bartolomé";
    "BLR" "by" Eu "Belarus" "Bielorrusia";
    "BLZ" "bz" Am "Belize" "Belice";
    "BOL" "bo" Am "Bolivia" "Bolivia";
    "BOT" "bw" Af "Botswana" "Botsuana";
    "BRB" "bb" Am "Barbados" "Barbados";
    "BRU" "bn" As "Brunei" "Brunéi";
    "BTN" "bt" As "Bhutan" "Bután";
    "BUL" "bg" Eu "Bulgaria" "Bulgaria";
    "BVT" "bv" Other "Bouvet Island" "Isla Bouvet";
    "CAB" "ao" Af "Cabinda" "Cabinda";
    "CAF" "cf" Af "Central African Republic" "República Centroafricana";
    "CAN" "ca" Am "Canada" "Canadá";
    "CBG" "kh" As "Cambodia" "Camboya";
    "CEU" "es" Af "Ceuta" "Ceuta";
    "CG7" "us" Am "Guantanamo Bay" "Bahía de Guantánamo";
    "CHL" "cl" Am "Chile" "Chile";
    "CHN" "cn" As "China" "China";
    "CHR" "cx" Oc "Christmas Island" "Isla de Navidad";
    "CKH" "ck" Oc "Cook Islands" "Islas Cook";
    "CLA" - Other "Clandestine" "Clandestina";
    "CLM" "co" Am "Colombia" "Colombia";
    "CLN" "lk" As "Sri Lanka" "Sri Lanka";
    "CME" "cm" Af "Cameroon" "Camerún";
    "CNR" "ic" Af "Canary Islands" "Islas Canarias";
    "COD" "cd" Af "DR Congo" "RD del Congo";
    "COG" "cg" Af "Republic of the Congo" "República del Congo";
    "COM" "km" Af "Comoros" "Comoras";
    "CPT" "cp" Am "Clipperton Island" "Isla Clipperton";
    "CPV" "cv" Af "Cape Verde" "Cabo Verde";
    "CRO" "tf" Other "Crozet Islands" "Islas Crozet";
    "CTI" "ci" Af "Ivory Coast" "Costa de Marfil";
    "CTR" "cr" Am "Costa Rica" "Costa Rica";
    "CUB" "cu" Am "Cuba" "Cuba";
    "CUW" "cw" Am "Curaçao" "Curazao";
    "CVA" "va" Eu "Vatican City" "Ciudad del Vaticano";
    "CYM" "ky" Am "Cayman Islands" "Islas Caimán";
    "CYP" "cy" Eu "Cyprus" "Chipre";
    "CZE" "cz" Eu "Czech Republic" "República Checa";
    "D" "de" Eu "Germany" "Alemania";
    "DJI" "dj" Af "Djibouti" "Yibuti";
    "DMA" "dm" Am "Dominica" "Dominica";
    "DNK" "dk" Eu "Denmark" "Dinamarca";
    "DOM" "do" Am "Dominican Republic" "República Dominicana";
    "E" "es" Eu "Spain" "España";
    "EGY" "eg" Af "Egypt" "Egipto";
    "EQA" "ec" Am "Ecuador" "Ecuador";
    "ERI" "er" Af "Eritrea" "Eritrea";
    "EST" "ee" Eu "Estonia" "Estonia";
    "ETH" "et" Af "Ethiopia" "Etiopía";
    "EUR" "tf" Af "Europa Island and Bassas da India" "Isla Europa y Bassas da India";
    "F" "fr" Eu "France" "Francia";
    "FIN" "fi" Eu "Finland" "Finlandia";
    "FJI" "fj" Oc "Fiji" "Fiyi";
    "FLK" "fk" Am "Falkland Islands" "Islas Malvinas";
    "FRO" "fo" Eu "Faroe Islands" "Islas Feroe";
    "FSM" "fm" Oc "Micronesia" "Micronesia";
    "G" "gb" Eu "United Kingdom" "Reino Unido";
    "GAB" "ga" Af "Gabon" "Gabón";
    "GEO" "ge" Me "Georgia" "Georgia";
    "GHA" "gh" Af "Ghana" "Ghana";
    "GIB" "gi" Eu "Gibraltar" "Gibraltar";
    "GLP" "gp" Am "Guadeloupe" "Guadalupe";
    "GMB" "gm" Af "Gambia" "Gambia";
    "GNB" "gw" Af "Guinea-Bissau" "Guinea-Bisáu";
    "GNE" "gq" Af "Equatorial Guinea" "Guinea Ecuatorial";
    "GPG" "ec" Am "Galápagos Islands" "Islas Galápagos";
    "GRC" "gr" Eu "Greece" "Grecia";
    "GRD" "gd" Am "Grenada" "Granada";
    "GRL" "gl" Am "Greenland" "Groenlandia";
    "GTM" "gt" Am "Guatemala" "Guatemala";
    "GUF" "gf" Am "French Guiana" "Guayana Francesa";
    "GUI" "gn" Af "Guinea" "Guinea";
    "GUM" "gu" Oc "Guam" "Guam";
    "GUY" "gy" Am "Guyana" "Guyana";
    "HKG" "hk" As "Hong Kong" "Hong Kong";
    "HMD" "hm" Other "Heard and McDonald Islands" "Islas Heard y McDonald";
    "HND" "hn" Am "Honduras" "Honduras";
    "HNG" "hu" Eu "Hungary" "Hungría";
    "HOL" "nl" Eu "Netherlands" "Países Bajos";
    "HRV" "hr" Eu "Croatia" "Croacia";
    "HTI" "ht" Am "Haiti" "Haití";
    "HWA" "us" Oc "Hawaii" "Hawái";
    "HWL" "um" Oc "Howland and Baker Islands" "Islas Howland y Baker";
    "I" "it" Eu "Italy" "Italia";
    "ICO" "cc" Oc "Cocos (Keeling) Islands" "Islas Cocos";
    "IND" "in" As "India" "India";
    "INS" "id" As "Indonesia" "Indonesia";
    "IRL" "ie" Eu "Ireland" "Irlanda";
    "IRN" "ir" Me "Iran" "Irán";
    "IRQ" "iq" Me "Iraq" "Irak";
    "ISL" "is" Eu "Iceland" "Islandia";
    "ISR" "il" Me "Israel" "Israel";
    "IW" - Other "International waters" "Aguas internacionales";
    "IWA" "jp" As "Ogasawara Islands" "Islas Ogasawara";
    "J" "jp" As "Japan" "Japón";
    "JAR" "um" Oc "Jarvis Island" "Isla Jarvis";
    "JDN" "tf" Af "Juan de Nova Island" "Isla Juan de Nova";
    "JMC" "jm" Am "Jamaica" "Jamaica";
    "JMY" "sj" Eu "Jan Mayen" "Jan Mayen";
    "JON" "um" Oc "Johnston Atoll" "Atolón Johnston";
    "JOR" "jo" Me "Jordan" "Jordania";
    "JUF" "cl" Am "Juan Fernández Islands" "Archipiélago Juan Fernández";
    "KAL" "ru" Eu "Kaliningrad" "Kaliningrado";
    "KAZ" "kz" As "Kazakhstan" "Kazajistán";
    "KEN" "ke" Af "Kenya" "Kenia";
    "KER" "tf" Other "Kerguelen Islands" "Islas Kerguelen";
    "KGZ" "kg" As "Kyrgyzstan" "Kirguistán";
    "KIR" "ki" Oc "Kiribati" "Kiribati";
    "KNA" "kn" Am "Saint Kitts and Nevis" "San Cristóbal y Nieves";
    "KOR" "kr" As "South Korea" "Corea del Sur";
    "KOS" "xk" Eu "Kosovo" "Kosovo";
    "KRE" "kp" As "North Korea" "Corea del Norte";
    "KWT" "kw" Me "Kuwait" "Kuwait";
    "LAO" "la" As "Laos" "Laos";
    "LBN" "lb" Me "Lebanon" "Líbano";
    "LBR" "lr" Af "Liberia" "Liberia";
    "LBY" "ly" Af "Libya" "Libia";
    "LCA" "lc" Am "Saint Lucia" "Santa Lucía";
    "LIE" "li" Eu "Liechtenstein" "Liechtenstein";
    "LSO" "ls" Af "Lesotho" "Lesoto";
    "LTU" "lt" Eu "Lithuania" "Lituania";
    "LUX" "lu" Eu "Luxembourg" "Luxemburgo";
    "LVA" "lv" Eu "Latvia" "Letonia";
    "MAC" "mo" As "Macao" "Macao";
    "MAF" "mf" Am "Saint Martin" "San Martín";
    "MAU" "mu" Af "Mauritius" "Mauricio";
    "MCO" "mc" Eu "Monaco" "Mónaco";
    "MDA" "md" Eu "Moldova" "Moldavia";
    "MDG" "mg" Af "Madagascar" "Madagascar";
    "MDR" "pt" Eu "Madeira" "Madeira";
    "MDW" "um" Oc "Midway Islands" "Islas Midway";
    "MEL" "es" Af "Melilla" "Melilla";
    "MEX" "mx" Am "Mexico" "México";
    "MHL" "mh" Oc "Marshall Islands" "Islas Marshall";
    "MKD" "mk" Eu "North Macedonia" "Macedonia del Norte";
    "MLA" "my" As "Malaysia" "Malasia";
    "MLD" "mv" As "Maldives" "Maldivas";
    "MLI" "ml" Af "Mali" "Malí";
    "MLT" "mt" Eu "Malta" "Malta";
    "MNE" "me" Eu "Montenegro" "Montenegro";
    "MNG" "mn" As "Mongolia" "Mongolia";
    "MOZ" "mz" Af "Mozambique" "Mozambique";
    "MRA" "mp" Oc "Northern Mariana Islands" "Islas Marianas del Norte";
    "MRC" "ma" Af "Morocco" "Marruecos";
    "MRN" "za" Other "Prince Edward Islands" "Islas Príncipe Eduardo";
    "MRT" "mq" Am "Martinique" "Martinica";
    "MSR" "ms" Am "Montserrat" "Montserrat";
    "MTN" "mr" Af "Mauritania" "Mauritania";
    "MWI" "mw" Af "Malawi" "Malaui";
    "MYA" "mm" As "Myanmar" "Myanmar";
    "MYT" "yt" Af "Mayotte" "Mayotte";
    "NCG" "ni" Am "Nicaragua" "Nicaragua";
    "NCL" "nc" Oc "New Caledonia" "Nueva Caledonia";
    "NFK" "nf" Oc "Norfolk Island" "Isla Norfolk";
    "NGR" "ne" Af "Niger" "Níger";
    "NIG" "ng" Af "Nigeria" "Nigeria";
    "NIU" "nu" Oc "Niue" "Niue";
    "NMB" "na" Af "Namibia" "Namibia";
    "NOR" "no" Eu "Norway" "Noruega";
    "NPL" "np" As "Nepal" "Nepal";
    "NRU" "nr" Oc "Nauru" "Nauru";
    "NZL" "nz" Oc "New Zealand" "Nueva Zelanda";
    "OCE" "pf" Oc "French Polynesia" "Polinesia Francesa";
    "OMA" "om" Me "Oman" "Omán";
    "PAK" "pk" As "Pakistan" "Pakistán";
    "PAQ" "cl" Oc "Easter Island" "Isla de Pascua";
    "PHL" "ph" As "Philippines" "Filipinas";
    "PHX" "ki" Oc "Phoenix Islands" "Islas Fénix";
    "PLM" "um" Oc "Palmyra Atoll" "Atolón Palmyra";
    "PLW" "pw" Oc "Palau" "Palaos";
    "PNG" "pg" Oc "Papua New Guinea" "Papúa Nueva Guinea";
    "PNR" "pa" Am "Panama" "Panamá";
    "POL" "pl" Eu "Poland" "Polonia";
    "POR" "pt" Eu "Portugal" "Portugal";
    "PRG" "py" Am "Paraguay" "Paraguay";
    "PRU" "pe" Am "Peru" "Perú";
    "PRV" "jp" As "Okinotorishima" "Okinotorishima";
    "PSE" "ps" Me "Palestine" "Palestina";
    "PTC" "pn" Oc "Pitcairn Islands" "Islas Pitcairn";
    "PTR" "pr" Am "Puerto Rico" "Puerto Rico";
    "QAT" "qa" Me "Qatar" "Catar";
    "REU" "re" Af "Réunion" "Reunión";
    "ROD" "mu" Af "Rodrigues" "Rodrigues";
    "ROU" "ro" Eu "Romania" "Rumania";
    "RRW" "rw" Af "Rwanda" "Ruanda";
    "RUS" "ru" Eu "Russia" "Rusia";
    "S" "se" Eu "Sweden" "Suecia";
    "SAP" "co" Am "San Andrés and Providencia" "San Andrés y Providencia";
    "SDN" "sd" Af "Sudan" "Sudán";
    "SEN" "sn" Af "Senegal" "Senegal";
    "SEY" "sc" Af "Seychelles" "Seychelles";
    "SGA" "gs" Other "South Georgia" "Georgia del Sur";
    "SHN" "sh-hl" Af "Saint Helena" "Santa Elena";
    "SLM" "sb" Oc "Solomon Islands" "Islas Salomón";
    "SLV" "sv" Am "El Salvador" "El Salvador";
    "SMA" "as" Oc "American Samoa" "Samoa Americana";
    "SMO" "ws" Oc "Samoa" "Samoa";
    "SMR" "sm" Eu "San Marino" "San Marino";
    "SNG" "sg" As "Singapore" "Singapur";
    "SOK" "aq" Other "South Orkney Islands" "Islas Orcadas del Sur";
    "SOM" "so" Af "Somalia" "Somalia";
    "SPM" "pm" Am "Saint Pierre and Miquelon" "San Pedro y Miquelón";
    "SRB" "rs" Eu "Serbia" "Serbia";
    "SRL" "sl" Af "Sierra Leone" "Sierra Leona";
    "SSD" "ss" Af "South Sudan" "Sudán del Sur";
    "SSI" "gs" Other "South Sandwich Islands" "Islas Sandwich del Sur";
    "STP" "st" Af "São Tomé and Príncipe" "Santo Tomé y Príncipe";
    "SUI" "ch" Eu "Switzerland" "Suiza";
    "SUR" "sr" Am "Suriname" "Surinam";
    "SVB" "sj" Eu "Svalbard" "Svalbard";
    "SVK" "sk" Eu "Slovakia" "Eslovaquia";
    "SVN" "si" Eu "Slovenia" "Eslovenia";
    "SWZ" "sz" Af "Eswatini" "Esuatini";
    "SXM" "sx" Am "Sint Maarten" "Sint Maarten";
    "SYR" "sy" Me "Syria" "Siria";
    "TCA" "tc" Am "Turks and Caicos Islands" "Islas Turcas y Caicos";
    "TCD" "td" Af "Chad" "Chad";
    "TGO" "tg" Af "Togo" "Togo";
    "THA" "th" As "Thailand" "Tailandia";
    "TJK" "tj" As "Tajikistan" "Tayikistán";
    "TKL" "tk" Oc "Tokelau" "Tokelau";
    "TKM" "tm" As "Turkmenistan" "Turkmenistán";
    "TLS" "tl" As "Timor-Leste" "Timor Oriental";
    "TON" "to" Oc "Tonga" "Tonga";
    "TRC" "sh-ta" Af "Tristan da Cunha" "Tristán de Acuña";
    "TRD" "tt" Am "Trinidad and Tobago" "Trinidad y Tobago";
    "TUN" "tn" Af "Tunisia" "Túnez";
    "TUR" "tr" Me "Turkey" "Turquía";
    "TUV" "tv" Oc "Tuvalu" "Tuvalu";
    "TWN" "tw" As "Taiwan" "Taiwán";
    "TZA" "tz" Af "Tanzania" "Tanzania";
    "UAE" "ae" Me "United Arab Emirates" "Emiratos Árabes Unidos";
    "UGA" "ug" Af "Uganda" "Uganda";
    "UKR" "ua" Eu "Ukraine" "Ucrania";
    "UN" "un" Other "United Nations" "Naciones Unidas";
    "URG" "uy" Am "Uruguay" "Uruguay";
    "USA" "us" Am "United States" "Estados Unidos";
    "UZB" "uz" As "Uzbekistan" "Uzbekistán";
    "VCT" "vc" Am "Saint Vincent and the Grenadines" "San Vicente y las Granadinas";
    "VEN" "ve" Am "Venezuela" "Venezuela";
    "VIR" "vi" Am "U.S. Virgin Islands" "Islas Vírgenes de EE. UU.";
    "VRG" "vg" Am "British Virgin Islands" "Islas Vírgenes Británicas";
    "VTN" "vn" As "Vietnam" "Vietnam";
    "VUT" "vu" Oc "Vanuatu" "Vanuatu";
    "WAK" "um" Oc "Wake Island" "Isla Wake";
    "WAL" "wf" Oc "Wallis and Futuna" "Wallis y Futuna";
    "XBY" - Af "Abyei" "Abyei";
    "XGZ" "ps" Me "Gaza Strip" "Franja de Gaza";
    "XSP" - As "Spratly Islands" "Islas Spratly";
    "XUU" "xx" Other "Unidentified" "Sin identificar";
    "XWB" "ps" Me "West Bank" "Cisjordania";
    "YEM" "ye" Me "Yemen" "Yemen";
    "ZMB" "zm" Af "Zambia" "Zambia";
    "ZWE" "zw" Af "Zimbabwe" "Zimbabue";
};

/// The country for an EiBi code, if known.
pub fn country(itu: &str) -> Option<&'static Country> {
    COUNTRIES.binary_search_by(|c| c.itu.cmp(itu)).ok().map(|i| &COUNTRIES[i])
}

/// The flag-icons key for an EiBi code.
pub fn flag(itu: &str) -> Option<&'static str> {
    country(itu).and_then(|c| c.flag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_and_unique() {
        assert!(COUNTRIES.windows(2).all(|w| w[0].itu < w[1].itu), "COUNTRIES must be sorted by code");
    }

    #[test]
    fn eibi_specials() {
        assert_eq!(flag("G"), Some("gb"));
        assert_eq!(flag("D"), Some("de"));
        assert_eq!(flag("HOL"), Some("nl"));
        assert_eq!(flag("KOS"), Some("xk"));
        assert_eq!(flag("CLA"), None);
        assert_eq!(flag("NOPE"), None);
        assert_eq!(country("B").map(|c| c.es), Some("Brasil"));
        assert_eq!(country("KRE").map(|c| c.region), Some(Region::As));
    }

    #[test]
    fn flags_are_flag_icons_keys() {
        // keys beyond plain ISO alpha-2 that flag-icons ships
        const EXTRAS: [&str; 3] = ["sh-ac", "sh-hl", "sh-ta"];
        for c in COUNTRIES {
            if let Some(f) = c.flag {
                let iso = f.len() == 2 && f.bytes().all(|b| b.is_ascii_lowercase());
                assert!(iso || EXTRAS.contains(&f), "{}: {f}", c.itu);
            }
        }
    }
}
