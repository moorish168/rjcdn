//! Static continent → country catalog for DNS lines / WAF geo pickers.
//!
//! The MaxMind MMDB tells us which country (and continent code) an IP belongs
//! to; this table drives the console tree ("check North America → expand to
//! US, CA, …") and expands a whole-continent WAF rule into country codes.
//!
//! It lists every country GeoLite2-Country can return (250 codes in the
//! 2026-11-13 build), each under the continent the database reports for it,
//! so a continent picked here matches exactly what the edge sees.

#[derive(Debug, Clone, Copy)]
pub struct ContinentDef {
    /// UN M49 / MaxMind style code: NA, EU, AS, AF, SA, OC, AN.
    pub code: &'static str,
    pub name_zh: &'static str,
    pub name_en: &'static str,
    pub countries: &'static [CountryDef],
}

#[derive(Debug, Clone, Copy)]
pub struct CountryDef {
    /// ISO 3166-1 alpha-2.
    pub code: &'static str,
    pub name_zh: &'static str,
    pub name_en: &'static str,
}

macro_rules! country {
    ($code:literal, $zh:literal, $en:literal) => {
        CountryDef {
            code: $code,
            name_zh: $zh,
            name_en: $en,
        }
    };
}

/// Continents aligned with MaxMind GeoLite2 `continent.code`, in the order the
/// console lists them.
pub const CONTINENTS: &[ContinentDef] = &[
    ContinentDef {
        code: "AS",
        name_zh: "亚洲",
        name_en: "Asia",
        countries: AS,
    },
    ContinentDef {
        code: "EU",
        name_zh: "欧洲",
        name_en: "Europe",
        countries: EU,
    },
    ContinentDef {
        code: "NA",
        name_zh: "北美洲",
        name_en: "North America",
        countries: NA,
    },
    ContinentDef {
        code: "SA",
        name_zh: "南美洲",
        name_en: "South America",
        countries: SA,
    },
    ContinentDef {
        code: "AF",
        name_zh: "非洲",
        name_en: "Africa",
        countries: AF,
    },
    ContinentDef {
        code: "OC",
        name_zh: "大洋洲",
        name_en: "Oceania",
        countries: OC,
    },
    ContinentDef {
        code: "AN",
        name_zh: "南极洲",
        name_en: "Antarctica",
        countries: AN,
    },
];

const NA: &[CountryDef] = &[
    country!("US", "美国", "United States"),
    country!("CA", "加拿大", "Canada"),
    country!("MX", "墨西哥", "Mexico"),
    country!("GT", "危地马拉", "Guatemala"),
    country!("BZ", "伯利兹", "Belize"),
    country!("SV", "萨尔瓦多", "El Salvador"),
    country!("HN", "洪都拉斯", "Honduras"),
    country!("NI", "尼加拉瓜", "Nicaragua"),
    country!("CR", "哥斯达黎加", "Costa Rica"),
    country!("PA", "巴拿马", "Panama"),
    country!("CU", "古巴", "Cuba"),
    country!("JM", "牙买加", "Jamaica"),
    country!("HT", "海地", "Haiti"),
    country!("DO", "多米尼加", "Dominican Republic"),
    country!("PR", "波多黎各", "Puerto Rico"),
    country!("TT", "特立尼达和多巴哥", "Trinidad and Tobago"),
    country!("BS", "巴哈马", "Bahamas"),
    country!("BB", "巴巴多斯", "Barbados"),
    country!("AI", "安圭拉", "Anguilla"),
    country!("AG", "安提瓜和巴布达", "Antigua and Barbuda"),
    country!("AW", "阿鲁巴", "Aruba"),
    country!("BM", "百慕大", "Bermuda"),
    country!("BQ", "荷兰加勒比区", "Bonaire, Sint Eustatius and Saba"),
    country!("VG", "英属维尔京群岛", "British Virgin Islands"),
    country!("KY", "开曼群岛", "Cayman Islands"),
    country!("CW", "库拉索", "Curaçao"),
    country!("DM", "多米尼克", "Dominica"),
    country!("GL", "格陵兰", "Greenland"),
    country!("GD", "格林纳达", "Grenada"),
    country!("GP", "瓜德罗普", "Guadeloupe"),
    country!("MQ", "马提尼克", "Martinique"),
    country!("MS", "蒙特塞拉特", "Montserrat"),
    country!("BL", "圣巴泰勒米", "Saint Barthélemy"),
    country!("KN", "圣基茨和尼维斯", "Saint Kitts and Nevis"),
    country!("LC", "圣卢西亚", "Saint Lucia"),
    country!("MF", "法属圣马丁", "Saint Martin"),
    country!("PM", "圣皮埃尔和密克隆群岛", "Saint Pierre and Miquelon"),
    country!(
        "VC",
        "圣文森特和格林纳丁斯",
        "Saint Vincent and the Grenadines"
    ),
    country!("SX", "荷属圣马丁", "Sint Maarten"),
    country!("TC", "特克斯和凯科斯群岛", "Turks and Caicos Islands"),
    country!("VI", "美属维尔京群岛", "U.S. Virgin Islands"),
];

const SA: &[CountryDef] = &[
    country!("BR", "巴西", "Brazil"),
    country!("AR", "阿根廷", "Argentina"),
    country!("CL", "智利", "Chile"),
    country!("CO", "哥伦比亚", "Colombia"),
    country!("PE", "秘鲁", "Peru"),
    country!("VE", "委内瑞拉", "Venezuela"),
    country!("EC", "厄瓜多尔", "Ecuador"),
    country!("BO", "玻利维亚", "Bolivia"),
    country!("PY", "巴拉圭", "Paraguay"),
    country!("UY", "乌拉圭", "Uruguay"),
    country!("GY", "圭亚那", "Guyana"),
    country!("SR", "苏里南", "Suriname"),
    country!("FK", "马尔维纳斯群岛（福克兰）", "Falkland Islands"),
    country!("GF", "法属圭亚那", "French Guiana"),
];

const EU: &[CountryDef] = &[
    country!("DE", "德国", "Germany"),
    country!("FR", "法国", "France"),
    country!("GB", "英国", "United Kingdom"),
    country!("IT", "意大利", "Italy"),
    country!("ES", "西班牙", "Spain"),
    country!("NL", "荷兰", "Netherlands"),
    country!("BE", "比利时", "Belgium"),
    country!("CH", "瑞士", "Switzerland"),
    country!("AT", "奥地利", "Austria"),
    country!("SE", "瑞典", "Sweden"),
    country!("NO", "挪威", "Norway"),
    country!("DK", "丹麦", "Denmark"),
    country!("FI", "芬兰", "Finland"),
    country!("IE", "爱尔兰", "Ireland"),
    country!("PT", "葡萄牙", "Portugal"),
    country!("PL", "波兰", "Poland"),
    country!("CZ", "捷克", "Czechia"),
    country!("RO", "罗马尼亚", "Romania"),
    country!("HU", "匈牙利", "Hungary"),
    country!("GR", "希腊", "Greece"),
    country!("RU", "俄罗斯", "Russia"),
    country!("UA", "乌克兰", "Ukraine"),
    country!("AL", "阿尔巴尼亚", "Albania"),
    country!("AD", "安道尔", "Andorra"),
    country!("BY", "白俄罗斯", "Belarus"),
    country!("BA", "波黑", "Bosnia and Herzegovina"),
    country!("BG", "保加利亚", "Bulgaria"),
    country!("HR", "克罗地亚", "Croatia"),
    country!("CY", "塞浦路斯", "Cyprus"),
    country!("EE", "爱沙尼亚", "Estonia"),
    country!("FO", "法罗群岛", "Faroe Islands"),
    country!("GI", "直布罗陀", "Gibraltar"),
    country!("GG", "根西岛", "Guernsey"),
    country!("IS", "冰岛", "Iceland"),
    country!("IM", "马恩岛", "Isle of Man"),
    country!("JE", "泽西岛", "Jersey"),
    country!("XK", "科索沃", "Kosovo"),
    country!("LV", "拉脱维亚", "Latvia"),
    country!("LI", "列支敦士登", "Liechtenstein"),
    country!("LT", "立陶宛", "Lithuania"),
    country!("LU", "卢森堡", "Luxembourg"),
    country!("MT", "马耳他", "Malta"),
    country!("MD", "摩尔多瓦", "Moldova"),
    country!("MC", "摩纳哥", "Monaco"),
    country!("ME", "黑山", "Montenegro"),
    country!("MK", "北马其顿", "North Macedonia"),
    country!("SM", "圣马力诺", "San Marino"),
    country!("RS", "塞尔维亚", "Serbia"),
    country!("SK", "斯洛伐克", "Slovakia"),
    country!("SI", "斯洛文尼亚", "Slovenia"),
    country!("SJ", "斯瓦尔巴和扬马延", "Svalbard and Jan Mayen"),
    country!("VA", "梵蒂冈", "Vatican City"),
    country!("AX", "奥兰群岛", "Åland Islands"),
];

const AS: &[CountryDef] = &[
    country!("CN", "中国", "China"),
    country!("HK", "中国香港", "Hong Kong"),
    country!("MO", "中国澳门", "Macao"),
    country!("TW", "中国台湾", "Taiwan"),
    country!("JP", "日本", "Japan"),
    country!("KR", "韩国", "South Korea"),
    country!("IN", "印度", "India"),
    country!("SG", "新加坡", "Singapore"),
    country!("MY", "马来西亚", "Malaysia"),
    country!("TH", "泰国", "Thailand"),
    country!("VN", "越南", "Vietnam"),
    country!("ID", "印度尼西亚", "Indonesia"),
    country!("PH", "菲律宾", "Philippines"),
    country!("PK", "巴基斯坦", "Pakistan"),
    country!("BD", "孟加拉国", "Bangladesh"),
    country!("SA", "沙特阿拉伯", "Saudi Arabia"),
    country!("AE", "阿联酋", "United Arab Emirates"),
    country!("IL", "以色列", "Israel"),
    country!("KZ", "哈萨克斯坦", "Kazakhstan"),
    country!("TR", "土耳其", "Türkiye"),
    country!("AF", "阿富汗", "Afghanistan"),
    country!("AM", "亚美尼亚", "Armenia"),
    country!("AZ", "阿塞拜疆", "Azerbaijan"),
    country!("BH", "巴林", "Bahrain"),
    country!("BT", "不丹", "Bhutan"),
    country!("IO", "英属印度洋领地", "British Indian Ocean Territory"),
    country!("BN", "文莱", "Brunei"),
    country!("KH", "柬埔寨", "Cambodia"),
    country!("CC", "科科斯（基林）群岛", "Cocos (Keeling) Islands"),
    country!("GE", "格鲁吉亚", "Georgia"),
    country!("IR", "伊朗", "Iran"),
    country!("IQ", "伊拉克", "Iraq"),
    country!("JO", "约旦", "Jordan"),
    country!("KW", "科威特", "Kuwait"),
    country!("KG", "吉尔吉斯斯坦", "Kyrgyzstan"),
    country!("LA", "老挝", "Laos"),
    country!("LB", "黎巴嫩", "Lebanon"),
    country!("MV", "马尔代夫", "Maldives"),
    country!("MN", "蒙古", "Mongolia"),
    country!("MM", "缅甸", "Myanmar"),
    country!("NP", "尼泊尔", "Nepal"),
    country!("KP", "朝鲜", "North Korea"),
    country!("OM", "阿曼", "Oman"),
    country!("PS", "巴勒斯坦", "Palestine"),
    country!("QA", "卡塔尔", "Qatar"),
    country!("LK", "斯里兰卡", "Sri Lanka"),
    country!("SY", "叙利亚", "Syria"),
    country!("TJ", "塔吉克斯坦", "Tajikistan"),
    country!("TM", "土库曼斯坦", "Turkmenistan"),
    country!("UZ", "乌兹别克斯坦", "Uzbekistan"),
    country!("YE", "也门", "Yemen"),
];

const AF: &[CountryDef] = &[
    country!("ZA", "南非", "South Africa"),
    country!("EG", "埃及", "Egypt"),
    country!("NG", "尼日利亚", "Nigeria"),
    country!("KE", "肯尼亚", "Kenya"),
    country!("MA", "摩洛哥", "Morocco"),
    country!("DZ", "阿尔及利亚", "Algeria"),
    country!("GH", "加纳", "Ghana"),
    country!("ET", "埃塞俄比亚", "Ethiopia"),
    country!("AO", "安哥拉", "Angola"),
    country!("BJ", "贝宁", "Benin"),
    country!("BW", "博茨瓦纳", "Botswana"),
    country!("BF", "布基纳法索", "Burkina Faso"),
    country!("BI", "布隆迪", "Burundi"),
    country!("CV", "佛得角", "Cabo Verde"),
    country!("CM", "喀麦隆", "Cameroon"),
    country!("CF", "中非", "Central African Republic"),
    country!("TD", "乍得", "Chad"),
    country!("KM", "科摩罗", "Comoros"),
    country!("CD", "刚果（金）", "Congo (DRC)"),
    country!("CG", "刚果（布）", "Congo Republic"),
    country!("DJ", "吉布提", "Djibouti"),
    country!("GQ", "赤道几内亚", "Equatorial Guinea"),
    country!("ER", "厄立特里亚", "Eritrea"),
    country!("SZ", "埃斯瓦蒂尼", "Eswatini"),
    country!("GA", "加蓬", "Gabon"),
    country!("GM", "冈比亚", "Gambia"),
    country!("GN", "几内亚", "Guinea"),
    country!("GW", "几内亚比绍", "Guinea-Bissau"),
    country!("CI", "科特迪瓦", "Ivory Coast"),
    country!("LS", "莱索托", "Lesotho"),
    country!("LR", "利比里亚", "Liberia"),
    country!("LY", "利比亚", "Libya"),
    country!("MG", "马达加斯加", "Madagascar"),
    country!("MW", "马拉维", "Malawi"),
    country!("ML", "马里", "Mali"),
    country!("MR", "毛利塔尼亚", "Mauritania"),
    country!("MU", "毛里求斯", "Mauritius"),
    country!("YT", "马约特", "Mayotte"),
    country!("MZ", "莫桑比克", "Mozambique"),
    country!("NA", "纳米比亚", "Namibia"),
    country!("NE", "尼日尔", "Niger"),
    country!("RW", "卢旺达", "Rwanda"),
    country!("RE", "留尼汪", "Réunion"),
    country!("SN", "塞内加尔", "Senegal"),
    country!("SC", "塞舌尔", "Seychelles"),
    country!("SL", "塞拉利昂", "Sierra Leone"),
    country!("SO", "索马里", "Somalia"),
    country!("SS", "南苏丹", "South Sudan"),
    country!("SH", "圣赫勒拿", "St. Helena"),
    country!("SD", "苏丹", "Sudan"),
    country!("ST", "圣多美和普林西比", "São Tomé and Príncipe"),
    country!("TZ", "坦桑尼亚", "Tanzania"),
    country!("TG", "多哥", "Togo"),
    country!("TN", "突尼斯", "Tunisia"),
    country!("UG", "乌干达", "Uganda"),
    country!("EH", "西撒哈拉", "Western Sahara"),
    country!("ZM", "赞比亚", "Zambia"),
    country!("ZW", "津巴布韦", "Zimbabwe"),
];

const OC: &[CountryDef] = &[
    country!("AU", "澳大利亚", "Australia"),
    country!("NZ", "新西兰", "New Zealand"),
    country!("FJ", "斐济", "Fiji"),
    country!("PG", "巴布亚新几内亚", "Papua New Guinea"),
    country!("AS", "美属萨摩亚", "American Samoa"),
    country!("CX", "圣诞岛", "Christmas Island"),
    country!("CK", "库克群岛", "Cook Islands"),
    country!("FM", "密克罗尼西亚联邦", "Federated States of Micronesia"),
    country!("PF", "法属波利尼西亚", "French Polynesia"),
    country!("GU", "关岛", "Guam"),
    country!("KI", "基里巴斯", "Kiribati"),
    country!("MH", "马绍尔群岛", "Marshall Islands"),
    country!("NR", "瑙鲁", "Nauru"),
    country!("NC", "新喀里多尼亚", "New Caledonia"),
    country!("NU", "纽埃", "Niue"),
    country!("NF", "诺福克岛", "Norfolk Island"),
    country!("MP", "北马里亚纳群岛", "Northern Mariana Islands"),
    country!("PW", "帕劳", "Palau"),
    country!("PN", "皮特凯恩群岛", "Pitcairn Islands"),
    country!("WS", "萨摩亚", "Samoa"),
    country!("SB", "所罗门群岛", "Solomon Islands"),
    country!("TL", "东帝汶", "Timor-Leste"),
    country!("TK", "托克劳", "Tokelau"),
    country!("TO", "汤加", "Tonga"),
    country!("TV", "图瓦卢", "Tuvalu"),
    country!(
        "UM",
        "美国本土外小岛屿",
        "United States Minor Outlying Islands"
    ),
    country!("VU", "瓦努阿图", "Vanuatu"),
    country!("WF", "瓦利斯和富图纳", "Wallis and Futuna"),
];

const AN: &[CountryDef] = &[
    country!("AQ", "南极洲", "Antarctica"),
    country!("BV", "布维岛", "Bouvet Island"),
    country!("TF", "法属南部领地", "French Southern Territories"),
    country!("HM", "赫德岛和麦克唐纳群岛", "Heard and McDonald Islands"),
    country!(
        "GS",
        "南乔治亚和南桑威奇群岛",
        "South Georgia and the South Sandwich Islands"
    ),
];

/// Look up continent code for an ISO country.
pub fn continent_for_country(country: &str) -> Option<&'static str> {
    let code = country.trim().to_ascii_uppercase();
    CONTINENTS
        .iter()
        .find(|cont| cont.countries.iter().any(|c| c.code == code))
        .map(|c| c.code)
}

/// Expand a continent code to ISO country codes.
pub fn countries_for_continent(continent: &str) -> Option<&'static [CountryDef]> {
    let code = continent.trim().to_ascii_uppercase();
    CONTINENTS
        .iter()
        .find(|c| c.code == code)
        .map(|c| c.countries)
}

fn find_country(code: &str) -> Option<&'static CountryDef> {
    CONTINENTS
        .iter()
        .flat_map(|cont| cont.countries.iter())
        .find(|c| c.code == code)
}

fn find_continent(code: &str) -> Option<&'static ContinentDef> {
    CONTINENTS.iter().find(|c| c.code == code)
}

fn is_code(s: &str) -> bool {
    s.len() == 2 && s.bytes().all(|b| b.is_ascii_uppercase())
}

/// The places one WAF 地区 rule covers.
///
/// Stored in `waf_geo_rules.value` as a comma list: whole continents as `@XX`
/// first, then ISO country codes, each part sorted — e.g. `@EU,CN,US`. The old
/// one-country form (`CN`) is the same encoding with a single entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AreaSet {
    pub continents: Vec<String>,
    pub countries: Vec<String>,
}

impl AreaSet {
    /// Read a stored value.
    ///
    /// Lenient on purpose: rows written before codes were checked against this
    /// catalog keep their well-formed codes, so the edge config does not change
    /// under them. New input goes through [`AreaSet::normalize`].
    pub fn parse(value: &str) -> Self {
        let mut continents = std::collections::BTreeSet::new();
        let mut countries = std::collections::BTreeSet::new();
        for raw in value.split(',') {
            let token = raw.trim().to_ascii_uppercase();
            if let Some(cont) = token.strip_prefix('@') {
                if find_continent(cont).is_some() {
                    continents.insert(cont.to_string());
                }
            } else if is_code(&token) {
                countries.insert(token);
            }
        }
        Self {
            continents: continents.into_iter().collect(),
            countries: countries.into_iter().collect(),
        }
    }

    /// Validate console input against the catalog and make it canonical: a
    /// continent whose every country is picked becomes `@XX`, and countries
    /// inside a picked continent are dropped.
    pub fn normalize(continents: &[String], countries: &[String]) -> Result<Self, String> {
        let mut conts = std::collections::BTreeSet::new();
        for raw in continents {
            let code = raw.trim().trim_start_matches('@').to_ascii_uppercase();
            if find_continent(&code).is_none() {
                return Err(format!("未知的洲代码：{}", raw.trim()));
            }
            conts.insert(code);
        }
        let mut picked = std::collections::BTreeSet::new();
        for raw in countries {
            let code = raw.trim().to_ascii_uppercase();
            if find_country(&code).is_none() {
                return Err(format!("未知的国家/地区代码：{}", raw.trim()));
            }
            picked.insert(code);
        }
        for cont in CONTINENTS {
            if !conts.contains(cont.code) && cont.countries.iter().all(|c| picked.contains(c.code))
            {
                conts.insert(cont.code.to_string());
            }
        }
        picked.retain(|code| continent_for_country(code).is_none_or(|cont| !conts.contains(cont)));
        if conts.is_empty() && picked.is_empty() {
            return Err("请至少选择一个洲或国家/地区".into());
        }
        Ok(Self {
            continents: conts.into_iter().collect(),
            countries: picked.into_iter().collect(),
        })
    }

    pub fn encode(&self) -> String {
        self.continents
            .iter()
            .map(|c| format!("@{c}"))
            .chain(self.countries.iter().cloned())
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Every ISO country code the set covers, sorted.
    pub fn expand(&self) -> Vec<String> {
        let mut out = std::collections::BTreeSet::new();
        for cont in &self.continents {
            if let Some(defs) = countries_for_continent(cont) {
                out.extend(defs.iter().map(|c| c.code.to_string()));
            }
        }
        out.extend(self.countries.iter().cloned());
        out.into_iter().collect()
    }

    /// Short Chinese description for audit logs and version notes.
    pub fn summary_zh(&self) -> String {
        const SHOWN: usize = 6;
        let names: Vec<String> = self
            .continents
            .iter()
            .map(|c| match find_continent(c) {
                Some(def) => format!("{}（全部）", def.name_zh),
                None => c.clone(),
            })
            .chain(self.countries.iter().map(|c| match find_country(c) {
                Some(def) => def.name_zh.to_string(),
                None => c.clone(),
            }))
            .collect();
        if names.len() > SHOWN {
            format!("{} 等 {} 项", names[..SHOWN].join("、"), names.len())
        } else {
            names.join("、")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_codes_are_unique_and_well_formed() {
        let mut seen = std::collections::HashSet::new();
        for cont in CONTINENTS {
            assert!(!cont.countries.is_empty(), "{} has no countries", cont.code);
            for c in cont.countries {
                assert!(is_code(c.code), "bad code {}", c.code);
                assert!(
                    !c.name_zh.is_empty() && !c.name_en.is_empty(),
                    "{} unnamed",
                    c.code
                );
                assert!(seen.insert(c.code), "{} listed twice", c.code);
            }
        }
        assert_eq!(seen.len(), 250, "every GeoLite2-Country code is listed");
        assert_eq!(
            continent_for_country("tr"),
            Some("AS"),
            "follows MaxMind, not convention"
        );
        assert_eq!(continent_for_country("RU"), Some("EU"));
    }

    #[test]
    fn normalize_folds_full_continents_and_drops_covered_countries() {
        let oc: Vec<String> = countries_for_continent("OC")
            .unwrap()
            .iter()
            .map(|c| c.code.to_string())
            .collect();
        let mut countries = oc.clone();
        countries.push("cn".into());
        let set = AreaSet::normalize(&["eu".into()], &[countries, vec!["DE".into()]].concat())
            .expect("valid input");
        assert_eq!(set.continents, vec!["EU", "OC"]);
        assert_eq!(set.countries, vec!["CN"], "DE is inside EU, OC folded");
        assert_eq!(set.encode(), "@EU,@OC,CN");
        assert_eq!(AreaSet::parse(&set.encode()), set);
        assert!(set.expand().contains(&"FJ".to_string()));
        assert!(set.expand().contains(&"DE".to_string()));
    }

    #[test]
    fn normalize_rejects_unknown_and_empty_input() {
        assert!(AreaSet::normalize(&[], &["UK".into()]).is_err());
        assert!(AreaSet::normalize(&["XX".into()], &[]).is_err());
        assert!(AreaSet::normalize(&[], &[]).is_err());
    }

    #[test]
    fn parse_keeps_legacy_single_codes() {
        assert_eq!(AreaSet::parse("CN").countries, vec!["CN"]);
        // Written before codes were checked: still shipped to the edge as-is.
        assert_eq!(AreaSet::parse("uk").expand(), vec!["UK"]);
        assert_eq!(AreaSet::parse("@AN").expand().len(), 5);
        assert!(AreaSet::parse("@ZZ,not-a-code").expand().is_empty());
    }

    #[test]
    fn summary_names_places() {
        let set = AreaSet::parse("@AS,US");
        assert_eq!(set.summary_zh(), "亚洲（全部）、美国");
    }
}
