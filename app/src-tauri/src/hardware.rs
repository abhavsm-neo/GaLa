//! Rough performance scores for processors and graphics cards, and the reading of Steam's
//! system requirements. Scores are estimates in thousands, on the scale of PassMark's
//! CPU Mark and G3D Mark, so a "6.0" graphics card is about a GTX 1050 Ti. They only need
//! to put parts in the right order; close calls between similar chips can go either way.

use serde::Serialize;

/// Graphics cards and their scores. Names are matched after `gpu_key` tidies them.
const GPUS: &[(&str, f64)] = &[
    // NVIDIA desktop
    ("gt 710", 0.6), ("gt 730", 0.9), ("gt 1030", 2.6), ("gtx 650", 1.8), ("gtx 650 ti", 2.6), ("gtx 660", 4.0),
    ("gtx 660 ti", 4.4), ("gtx 670", 5.2), ("gtx 680", 5.6), ("gtx 690", 6.5), ("gtx 750", 3.4), ("gtx 750 ti", 3.9),
    ("gtx 760", 4.9), ("gtx 770", 6.0), ("gtx 780", 7.4), ("gtx 780 ti", 8.6), ("gtx titan", 8.0), ("gtx 950", 5.2),
    ("gtx 960", 5.9), ("gtx 970", 9.6), ("gtx 980", 11.1), ("gtx 980 ti", 13.5), ("gtx 1050", 4.9), ("gtx 1050 ti", 6.0),
    ("gtx 1060", 10.0), ("gtx 1070", 13.4), ("gtx 1070 ti", 14.6), ("gtx 1080", 15.4), ("gtx 1080 ti", 18.3),
    ("gtx 1630", 4.5), ("gtx 1650", 7.8), ("gtx 1650 super", 10.0), ("gtx 1660", 11.4), ("gtx 1660 super", 12.6),
    ("gtx 1660 ti", 12.8), ("rtx 2060", 14.0), ("rtx 2060 super", 16.4), ("rtx 2070", 16.5), ("rtx 2070 super", 18.2),
    ("rtx 2080", 19.0), ("rtx 2080 super", 19.9), ("rtx 2080 ti", 21.8), ("rtx 3050", 12.8), ("rtx 3060", 17.0),
    ("rtx 3060 ti", 20.2), ("rtx 3070", 22.4), ("rtx 3070 ti", 23.5), ("rtx 3080", 24.7), ("rtx 3080 ti", 26.8),
    ("rtx 3090", 26.6), ("rtx 3090 ti", 29.5), ("rtx 4060", 19.6), ("rtx 4060 ti", 22.6), ("rtx 4070", 26.8),
    ("rtx 4070 super", 30.0), ("rtx 4070 ti", 31.5), ("rtx 4070 ti super", 32.0), ("rtx 4080", 34.6),
    ("rtx 4080 super", 35.0), ("rtx 4090", 38.5), ("rtx 5050", 18.0), ("rtx 5060", 23.0), ("rtx 5060 ti", 25.0),
    ("rtx 5070", 30.0), ("rtx 5070 ti", 35.0), ("rtx 5080", 37.0), ("rtx 5090", 45.0),
    // NVIDIA laptop
    ("mx150", 2.4), ("mx250", 2.8), ("mx350", 2.9), ("mx450", 4.4), ("mx550", 5.0), ("mx570", 6.0),
    ("gtx 1050 laptop", 4.5), ("gtx 1050 ti laptop", 5.4), ("gtx 1060 laptop", 8.8), ("gtx 1070 laptop", 11.5),
    ("gtx 1080 laptop", 13.5), ("gtx 1650 laptop", 7.0), ("gtx 1650 ti laptop", 7.3), ("gtx 1660 ti laptop", 10.5),
    ("rtx 2050 laptop", 6.5), ("rtx 2060 laptop", 12.5), ("rtx 2070 laptop", 14.0), ("rtx 2070 super laptop", 15.0),
    ("rtx 2080 laptop", 16.0), ("rtx 2080 super laptop", 17.0), ("rtx 3050 laptop", 10.0), ("rtx 3050 ti laptop", 10.8),
    ("rtx 3060 laptop", 15.5), ("rtx 3070 laptop", 18.0), ("rtx 3070 ti laptop", 19.0), ("rtx 3080 laptop", 19.5),
    ("rtx 3080 ti laptop", 22.0), ("rtx 4050 laptop", 15.5), ("rtx 4060 laptop", 17.5), ("rtx 4070 laptop", 19.5),
    ("rtx 4080 laptop", 26.0), ("rtx 4090 laptop", 28.0), ("rtx 5060 laptop", 20.0), ("rtx 5070 laptop", 22.0),
    ("rtx 5070 ti laptop", 25.0), ("rtx 5080 laptop", 29.0), ("rtx 5090 laptop", 32.0),
    // AMD desktop
    ("hd 7750", 2.1), ("hd 7770", 2.6), ("hd 7790", 3.4), ("hd 7850", 3.9), ("hd 7870", 4.6), ("hd 7950", 5.2),
    ("hd 7970", 6.0), ("r7 250", 1.6), ("r7 260x", 2.9), ("r7 265", 4.0), ("r7 360", 3.0), ("r7 370", 4.0),
    ("r9 270", 4.3), ("r9 270x", 4.8), ("r9 280", 5.4), ("r9 280x", 6.0), ("r9 285", 5.5), ("r9 290", 7.8),
    ("r9 290x", 8.2), ("r9 380", 5.6), ("r9 380x", 6.0), ("r9 390", 8.5), ("r9 390x", 8.8), ("r9 fury", 9.5),
    ("r9 fury x", 10.0), ("rx 460", 4.2), ("rx 470", 7.0), ("rx 480", 8.3), ("rx 550", 2.8), ("rx 560", 4.0),
    ("rx 570", 7.2), ("rx 580", 8.7), ("rx 590", 9.3), ("rx vega 56", 12.0), ("rx vega 64", 13.2), ("rx 5500 xt", 9.0), ("rx 5600 xt", 12.7), ("rx 5700", 14.0), ("rx 5700 xt", 15.4), ("rx 6400", 6.5),
    ("rx 6500 xt", 7.2), ("rx 6600", 15.0), ("rx 6600 xt", 16.2), ("rx 6650 xt", 16.7), ("rx 6700", 17.5),
    ("rx 6700 xt", 19.5), ("rx 6750 xt", 20.0), ("rx 6800", 22.5), ("rx 6800 xt", 25.0), ("rx 6900 xt", 26.3),
    ("rx 6950 xt", 27.5), ("rx 7600", 17.0), ("rx 7600 xt", 17.5), ("rx 7700 xt", 22.5), ("rx 7800 xt", 25.5),
    ("rx 7900 gre", 27.0), ("rx 7900 xt", 29.5), ("rx 7900 xtx", 31.5), ("rx 9060 xt", 24.0), ("rx 9070", 30.0),
    ("rx 9070 xt", 33.0), ("vii", 16.0),
    // AMD laptop and built-in graphics
    ("rx 5500m", 7.5), ("rx 5600m", 10.0), ("rx 6600m", 14.5), ("rx 6700m", 16.5), ("rx 6800m", 19.0),
    ("rx 7600s", 14.0), ("rx 7600m xt", 17.0), ("vega 8", 2.2), ("vega 11", 2.9), ("680m", 6.0),
    ("760m", 6.0), ("780m", 7.5), ("880m", 8.0), ("890m", 9.0),
    // Intel
    ("arc a310", 5.0), ("arc a380", 6.5), ("arc a580", 12.0), ("arc a750", 14.5), ("arc a770", 15.5),
    ("arc b570", 18.0), ("arc b580", 19.5), ("hd 4000", 0.6), ("hd 4600", 0.8), ("hd 520", 0.95), ("hd 530", 1.1),
    ("hd 620", 1.0), ("hd 630", 1.35), ("uhd 620", 1.0), ("uhd 630", 1.4), ("uhd 730", 1.3), ("uhd 770", 1.6),
    ("uhd", 1.2), ("iris plus", 1.8), ("iris xe", 2.5), ("arc", 5.5), ("arc 140v", 7.0),
];

/// Lower case, without brand words and memory sizes, with "laptop" marking mobile chips.
pub fn gpu_key(name: &str) -> String {
    let lower = name.to_lowercase().replace(['(', ')', '®', '™', '-'], " ");
    let mobile = ["laptop", "mobile", "max q", "max-q", "notebook"].iter().any(|word| lower.contains(word));
    let words: Vec<&str> = lower
        .split_whitespace()
        .filter(|word| {
            !matches!(*word, "nvidia" | "geforce" | "amd" | "ati" | "radeon" | "intel" | "graphics" | "gpu" | "series" | "laptop"
                | "mobile" | "max" | "q" | "notebook" | "with" | "or" | "equivalent" | "better" | "card" | "dedicated" | "video")
                && !word.ends_with("gb")
                && !word.ends_with("mb")
        })
        .collect();
    let mut key = words.join(" ");
    // "GTX1060" and "RX580" are written without spaces surprisingly often.
    for prefix in ["gtx", "rtx", "rx", "gt", "hd", "r9", "r7", "uhd", "mx"] {
        if let Some(rest) = key.strip_prefix(prefix).filter(|rest| rest.starts_with(|c: char| c.is_ascii_digit())) {
            key = format!("{prefix} {rest}");
        }
    }
    if mobile { format!("{key} laptop") } else { key }
}

/// The best known match: the longest table name found in the key as whole words.
pub fn gpu_score(name: &str) -> Option<f64> {
    let key = gpu_key(name);
    let padded = format!(" {key} ");
    let (model, score) = GPUS.iter().filter(|(model, _)| padded.contains(&format!(" {model} "))).max_by_key(|(model, _)| model.len())?;
    // A laptop chip missing from the table is a bit slower than the desktop one of the same name.
    let desktop_part = ["gtx ", "rtx ", "rx "].iter().any(|family| model.starts_with(family)) && !model.ends_with("laptop");
    Some(if key.ends_with(" laptop") && desktop_part { score * 0.8 } else { *score })
}

/// Intel Core i3, i5, i7 and i9 desktop chips without a K, by generation.
const INTEL: &[(u32, [f64; 4])] = &[
    (2, [3.0, 4.3, 5.4, 5.4]), (3, [3.2, 4.6, 6.4, 6.4]), (4, [3.4, 5.0, 7.0, 7.0]), (5, [3.5, 5.3, 7.2, 7.2]),
    (6, [4.1, 5.6, 8.0, 8.0]), (7, [4.4, 6.2, 8.6, 8.6]), (8, [6.0, 9.2, 12.4, 12.4]), (9, [6.6, 9.6, 13.0, 16.5]),
    (10, [8.7, 12.3, 16.8, 21.0]), (11, [9.0, 17.0, 21.9, 22.8]), (12, [14.3, 19.5, 30.8, 36.6]),
    (13, [14.8, 25.0, 39.0, 50.0]), (14, [15.2, 25.5, 44.0, 52.0]),
];

/// Per core, Ryzen series by their first digit. Zen, Zen+, Zen 2, Zen 3, Zen 4, Zen 5.
fn ryzen_per_core(series: u32) -> f64 {
    match series {
        1 => 2.05,
        2 => 2.25,
        3 | 4 => 3.0,
        5 | 6 => 3.9,
        7 | 8 => 4.9,
        _ => 5.4,
    }
}

fn number_after<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    let start = text.find(marker)? + marker.len();
    let rest = text[start..].trim_start_matches([' ', '-']);
    let end = rest.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(rest.len());
    Some(&rest[..end]).filter(|model| model.starts_with(|c: char| c.is_ascii_digit()))
}

fn intel_core(lower: &str) -> Option<f64> {
    let tier = ["i3", "i5", "i7", "i9"].iter().position(|tier| lower.contains(tier))?;
    let marker = ["i3", "i5", "i7", "i9"][tier];
    let model = number_after(lower, marker)?;
    let digits: String = model.chars().take_while(char::is_ascii_digit).collect();
    let suffix: String = model.chars().skip_while(char::is_ascii_digit).collect();
    let generation: u32 = match digits.len() {
        5 => digits[..2].parse().ok()?,
        4 => digits[..1].parse().ok()?,
        // First-generation chips like the i7-920.
        3 => return Some([1.8, 2.6, 3.4, 3.4][tier]),
        _ => return None,
    };
    let base = INTEL.iter().find(|(g, _)| *g == generation)?.1[tier];
    let factor = match suffix.as_str() {
        "k" | "kf" | "ks" => 1.12,
        "hx" => 1.05,
        "h" | "hk" | "hq" | "hs" => 0.85,
        s if s.starts_with('u') || s.starts_with('y') || s.starts_with('g') || s == "p" => 0.55,
        _ => 1.0,
    };
    Some(base * factor)
}

fn core_ultra(lower: &str) -> Option<f64> {
    let tier: usize = lower.split("ultra").nth(1)?.trim().chars().next()?.to_digit(10)? as usize;
    let model = number_after(lower, &format!("ultra {tier}"))?;
    let desktop = model.ends_with('k') || model.ends_with("kf") || (model.len() == 3 && model.chars().all(|c| c.is_ascii_digit()));
    let second = model.starts_with('2');
    let index = match tier { 5 => 0, 7 => 1, _ => 2 };
    Some(match (second, desktop) {
        (true, true) => [43.0, 58.0, 67.0][index],
        (true, false) => [19.0, 24.0, 28.0][index],
        (false, _) => [21.0, 25.0, 29.0][index],
    })
}

fn ryzen(lower: &str) -> Option<f64> {
    let tier: u32 = lower.split("ryzen").nth(1)?.trim().trim_start_matches('r').chars().next()?.to_digit(10)?;
    let model = number_after(lower, &format!("ryzen {tier}"))
        .or_else(|| number_after(lower, &format!("ryzen r{tier}")))
        .or_else(|| number_after(lower, &format!("ryzen {tier} pro")))?;
    let digits: String = model.chars().take_while(char::is_ascii_digit).collect();
    let suffix: String = model.chars().skip_while(char::is_ascii_digit).collect();
    if digits.len() != 4 {
        return None;
    }
    let series = digits[..1].parse().ok()?;
    let second: u32 = digits[1..2].parse().ok()?;
    let cores = match tier {
        9 => if digits[1..3] == *"95" { 16.0 } else { 12.0 },
        7 => 8.0,
        5 if series <= 2 && second <= 5 => 4.0,
        5 => 6.0,
        _ => 4.0,
    };
    let factor = match suffix.as_str() {
        "h" | "hs" | "hx" => 0.85,
        "u" => 0.6,
        _ => 1.0,
    };
    Some(cores * ryzen_per_core(series) * factor)
}

/// "Dual Core 2.4 GHz" and similar: a rough figure from core count and clock speed.
fn generic_cpu(lower: &str) -> Option<f64> {
    let cores = if lower.contains("octa") || lower.contains("8 core") || lower.contains("8-core") {
        8.0
    } else if lower.contains("hexa") || lower.contains("6 core") || lower.contains("6-core") || lower.contains("six") {
        6.0
    } else if lower.contains("quad") || lower.contains("4 core") || lower.contains("4-core") {
        4.0
    } else if lower.contains("dual") || lower.contains("2 core") || lower.contains("2-core") {
        2.0
    } else {
        return None;
    };
    let ghz = lower
        .split("ghz")
        .next()
        .and_then(|before| before.split_whitespace().last())
        .and_then(|number| number.trim_end_matches('+').parse::<f64>().ok())
        .unwrap_or(2.5);
    Some(cores * ghz * 0.55)
}

const OLDER_CPUS: &[(&str, f64)] = &[
    ("core 2 duo", 1.2), ("core2 duo", 1.2), ("core 2 quad", 2.9), ("core2 quad", 2.9), ("pentium", 2.5),
    ("celeron", 1.4), ("athlon ii", 1.6), ("athlon x4", 3.0), ("athlon 64", 0.8), ("phenom ii x6", 4.5),
    ("phenom ii x4", 3.0), ("phenom", 2.4), ("fx 9590", 6.8), ("fx 8350", 6.0), ("fx 8320", 5.7), ("fx 8300", 5.5),
    ("fx 6350", 4.5), ("fx 6300", 4.2), ("fx 4350", 3.3), ("fx 4300", 3.1), ("a10", 3.0), ("a8", 2.6),
];

pub fn cpu_score(name: &str) -> Option<f64> {
    let lower = name.to_lowercase().replace(['®', '™', '(', ')'], " ").replace("-", " ").replace("core tm", "core");
    let lower = lower.split_whitespace().collect::<Vec<_>>().join(" ");
    if lower.contains("ultra") {
        if let Some(score) = core_ultra(&lower) {
            return Some(score);
        }
    }
    intel_core(&lower)
        .or_else(|| ryzen(&lower))
        .or_else(|| OLDER_CPUS.iter().filter(|(model, _)| lower.contains(model)).max_by_key(|(model, _)| model.len()).map(|(_, s)| *s))
        .or_else(|| generic_cpu(&lower))
}

/// One level of a game's requirements, read from Steam's text.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Level {
    pub os: Option<String>,
    pub cpu: Option<String>,
    pub memory_gb: Option<f64>,
    pub gpu: Option<String>,
    pub vram_gb: Option<f64>,
    pub directx: Option<u32>,
    pub storage_gb: Option<f64>,
    pub lines: Vec<String>,
}

fn size_gb(text: &str) -> Option<f64> {
    let lower = text.to_lowercase();
    for (unit, scale) in [("gb", 1.0), ("mb", 1.0 / 1024.0), ("tb", 1024.0)] {
        if let Some(at) = lower.find(unit) {
            let number: String = lower[..at].trim_end().chars().rev().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ',').collect();
            let number: String = number.chars().rev().collect::<String>().replace(',', ".");
            if let Ok(value) = number.parse::<f64>() {
                return Some(value * scale);
            }
        }
    }
    None
}

/// Steam gives HTML like `<strong>Processor:</strong> Intel Core i5-6600K<br>`.
pub fn parse_level(html: &str) -> Level {
    let text = html
        .replace("<br>", "\n").replace("<br/>", "\n").replace("<br />", "\n").replace("<li>", "\n").replace("</li>", "\n");
    let text = crate::news::plain_text_lines(&text);
    let mut level = Level::default();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Some((label, value)) = line.split_once(':') else { continue };
        let label = label.trim().trim_end_matches('*').trim().to_lowercase();
        let value = value.trim().to_string();
        if value.is_empty() {
            continue;
        }
        level.lines.push(format!("{}: {value}", label_case(&label)));
        match label.as_str() {
            "os" | "os version" | "operating system" => level.os = Some(value),
            "processor" | "cpu" => level.cpu = Some(value),
            "memory" | "ram" | "system memory" => level.memory_gb = size_gb(&value),
            "graphics" | "video card" | "video" | "gpu" | "graphics card" => {
                let lower = value.to_lowercase();
                if lower.contains("vram") || lower.contains("video memory") || gpu_alternatives(&value).iter().all(|alt| gpu_score(alt).is_none()) {
                    level.vram_gb = size_gb(&value);
                }
                level.gpu = Some(value);
            }
            "directx" => level.directx = first_number(&value).filter(|version| (7..=12).contains(version)),
            "storage" | "hard drive" | "hard disk space" | "disk space" | "hdd" => level.storage_gb = size_gb(&value),
            _ => {}
        }
    }
    level
}

/// "Version 12" gives 12, "9.0c" gives 9.
fn first_number(text: &str) -> Option<u32> {
    let start = text.find(|c: char| c.is_ascii_digit())?;
    text[start..].chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()
}

fn label_case(label: &str) -> String {
    match label {
        "os" => "OS".into(),
        "directx" => "DirectX".into(),
        other => {
            let mut chars = other.chars();
            chars.next().map_or(String::new(), |first| first.to_uppercase().collect::<String>() + chars.as_str())
        }
    }
}

/// "GTX 1060 6GB or Radeon RX 580 / Arc A380" names three cards, any of which is enough.
pub fn gpu_alternatives(text: &str) -> Vec<String> {
    alternatives(text)
}

fn alternatives(text: &str) -> Vec<String> {
    text.replace(" or ", "|").replace(" OR ", "|").replace(" Or ", "|").replace('/', "|").replace(',', "|").replace(';', "|")
        .split('|')
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

/// The lowest score among the parts a requirement names, since any of them is enough.
pub fn required_cpu(text: &str) -> Option<f64> {
    alternatives(text).iter().filter_map(|alt| cpu_score(alt)).min_by(f64::total_cmp).or_else(|| cpu_score(text))
}

pub fn required_gpu(text: &str) -> Option<f64> {
    alternatives(text).iter().filter_map(|alt| gpu_score(alt)).min_by(f64::total_cmp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_graphics_cards() {
        assert_eq!(gpu_score("NVIDIA GeForce RTX 3050 Laptop GPU"), Some(10.0));
        assert_eq!(gpu_score("GeForce GTX 1060 6GB"), Some(10.0));
        assert_eq!(gpu_score("NVIDIA GeForce GTX 1050 Ti"), Some(6.0));
        assert_eq!(gpu_score("AMD Radeon RX 5700 XT"), Some(15.4));
        assert_eq!(gpu_score("Radeon RX 580 8GB"), Some(8.7));
        assert_eq!(gpu_score("GeForce RTX 2060 SUPER"), Some(16.4));
        assert_eq!(gpu_score("Arc A770"), Some(15.5));
        assert_eq!(gpu_score("GTX1060"), Some(10.0));
        assert_eq!(gpu_score("Intel(R) UHD Graphics"), Some(1.2));
        assert_eq!(gpu_score("NVIDIA GeForce RTX 4070 Ti SUPER"), Some(32.0));
        assert!(gpu_score("1GB VRAM").is_none());
        // A laptop chip not in the table falls back to a slower desktop score.
        assert_eq!(gpu_score("GeForce RTX 3090 Laptop"), Some(26.6 * 0.8));
        assert_eq!(gpu_score("AMD Radeon VII"), Some(16.0));
        assert_eq!(gpu_score("AMD Radeon 780M"), Some(7.5));
        assert_eq!(gpu_score("Intel Arc A380 Graphics"), Some(6.5));
    }

    #[test]
    fn scores_processors() {
        assert!((cpu_score("12th Gen Intel(R) Core(TM) i5-12450H").unwrap() - 19.5 * 0.85).abs() < 0.01);
        assert!((cpu_score("Intel Core i5-6600K").unwrap() - 5.6 * 1.12).abs() < 0.01);
        assert!((cpu_score("AMD Ryzen R5 1600 processor").unwrap() - 12.3).abs() < 0.01);
        assert!((cpu_score("Ryzen 5 3600").unwrap() - 18.0).abs() < 0.01);
        assert!((cpu_score("Ryzen 7 7800X3D").unwrap() - 39.2).abs() < 0.01);
        assert!((cpu_score("Core i7-12700").unwrap() - 30.8).abs() < 0.01);
        assert!((cpu_score("Ryzen 9 7950X").unwrap() - 16.0 * 4.9).abs() < 0.01);
        assert!((cpu_score("Dual Core 2.4 GHz").unwrap() - 2.64).abs() < 0.01);
        assert_eq!(cpu_score("AMD FX-8350"), Some(6.0));
        assert!(cpu_score("Intel Core Ultra 7 155H").is_some());
        assert!(cpu_score("A potato").is_none());
    }

    #[test]
    fn reads_steam_requirements() {
        let html = "<strong>Minimum:</strong><br><ul class=\"bb_ul\"><li>Requires a 64-bit processor and operating system<br></li>\
            <li><strong>OS:</strong> 64-bit Windows 10<br></li><li><strong>Processor:</strong> Core i7-6700 or Ryzen 5 1600<br></li>\
            <li><strong>Memory:</strong> 12 GB RAM<br></li><li><strong>Graphics:</strong> GeForce GTX 1060 6GB or Radeon RX 580 8GB or Arc A380<br></li>\
            <li><strong>DirectX:</strong> Version 12<br></li><li><strong>Storage:</strong> 70 GB available space</li></ul>";
        let level = parse_level(html);
        assert_eq!(level.memory_gb, Some(12.0));
        assert_eq!(level.storage_gb, Some(70.0));
        assert_eq!(level.directx, Some(12));
        assert_eq!(level.vram_gb, None, "a named card isn't a memory requirement");
        assert_eq!(required_gpu(level.gpu.as_deref().unwrap()), Some(6.5));
        assert!((required_cpu(level.cpu.as_deref().unwrap()).unwrap() - 8.0).abs() < 0.01);

        let hades = parse_level("<strong>OS *:</strong> Windows 7 SP1<br><strong>Processor:</strong> Dual Core 2.4 GHz<br>\
            <strong>Memory:</strong> 4 GB RAM<br><strong>Graphics:</strong> 1GB VRAM / DirectX 10+ support<br><strong>Storage:</strong> 15 GB available space");
        assert_eq!(hades.vram_gb, Some(1.0));
        assert_eq!(hades.memory_gb, Some(4.0));
        assert_eq!(hades.os.as_deref(), Some("Windows 7 SP1"));
    }
}
