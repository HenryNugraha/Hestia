// Unicode tables are generated from Python's Unicode 15.0 data and embedded as a compact asset.
#![allow(clippy::type_complexity)]

use std::sync::OnceLock;

const DATA: &[u8] = include_bytes!("unicode-tables.bin");

struct Tables {
    casefold: Vec<(u32, Vec<u32>)>,
    decimal: Vec<(u32, u8)>,
    decomp: Vec<(u32, Vec<u32>)>,
    ccc: Vec<(u32, u8)>,
    composition: Vec<(u64, u32)>,
    alnum_ranges: Vec<(u32, u32)>,
    space_ranges: Vec<(u32, u32)>,
    html5: Vec<(String, String)>,
    invalid_charrefs: Vec<(u32, u32)>,
}

struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], &'static str> {
        let end = self.position.checked_add(length).ok_or("table offset overflow")?;
        let bytes = self.data.get(self.position..end).ok_or("truncated table")?;
        self.position = end;
        Ok(bytes)
    }

    fn u8(&mut self) -> Result<u8, &'static str> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, &'static str> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Result<u32, &'static str> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64, &'static str> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn count(&mut self) -> Result<usize, &'static str> {
        usize::try_from(self.u32()?).map_err(|_| "table count overflow")
    }
}

fn read_variable_table(reader: &mut Reader<'_>) -> Result<Vec<(u32, Vec<u32>)>, &'static str> {
    let count = reader.count()?;
    let mut table = Vec::with_capacity(count);
    for _ in 0..count {
        let key = reader.u32()?;
        let length = usize::from(reader.u16()?);
        let mut values = Vec::with_capacity(length);
        for _ in 0..length {
            values.push(reader.u32()?);
        }
        table.push((key, values));
    }
    Ok(table)
}

fn read_scalar_u8_table(reader: &mut Reader<'_>) -> Result<Vec<(u32, u8)>, &'static str> {
    let count = reader.count()?;
    let mut table = Vec::with_capacity(count);
    for _ in 0..count {
        table.push((reader.u32()?, reader.u8()?));
    }
    Ok(table)
}

fn read_pair_u32_table(reader: &mut Reader<'_>) -> Result<Vec<(u32, u32)>, &'static str> {
    let count = reader.count()?;
    let mut table = Vec::with_capacity(count);
    for _ in 0..count {
        table.push((reader.u32()?, reader.u32()?));
    }
    Ok(table)
}

fn read_html_table(reader: &mut Reader<'_>) -> Result<Vec<(String, String)>, &'static str> {
    let count = reader.count()?;
    let mut table = Vec::with_capacity(count);
    for _ in 0..count {
        let key_length = usize::from(reader.u16()?);
        let value_length = usize::from(reader.u16()?);
        let key = std::str::from_utf8(reader.take(key_length)?)
            .map_err(|_| "invalid HTML entity name")?
            .to_owned();
        let value = std::str::from_utf8(reader.take(value_length)?)
            .map_err(|_| "invalid HTML entity value")?
            .to_owned();
        table.push((key, value));
    }
    Ok(table)
}

fn parse_tables(data: &[u8]) -> Result<Tables, &'static str> {
    let mut reader = Reader::new(data);
    if reader.take(8)? != b"HGBU\x01\x00\x00\x00" {
        return Err("invalid Unicode table header");
    }
    let tables = Tables {
        casefold: read_variable_table(&mut reader)?,
        decimal: read_scalar_u8_table(&mut reader)?,
        decomp: read_variable_table(&mut reader)?,
        ccc: read_scalar_u8_table(&mut reader)?,
        composition: {
            let count = reader.count()?;
            let mut table = Vec::with_capacity(count);
            for _ in 0..count {
                table.push((reader.u64()?, reader.u32()?));
            }
            table
        },
        alnum_ranges: read_pair_u32_table(&mut reader)?,
        space_ranges: read_pair_u32_table(&mut reader)?,
        html5: read_html_table(&mut reader)?,
        invalid_charrefs: read_pair_u32_table(&mut reader)?,
    };
    if reader.position != data.len() {
        return Err("trailing Unicode table data");
    }
    Ok(tables)
}

static TABLES: OnceLock<Result<Tables, &'static str>> = OnceLock::new();

fn tables() -> &'static Tables {
    TABLES
        .get_or_init(|| parse_tables(DATA))
        .as_ref()
        .expect("embedded Unicode tables are invalid")
}

fn lookup<T: Copy>(table: &[(u32, T)], codepoint: u32) -> Option<T> {
    table
        .binary_search_by_key(&codepoint, |(key, _)| *key)
        .ok()
        .map(|index| table[index].1)
}

fn lookup_values(table: &[(u32, Vec<u32>)], codepoint: u32) -> Option<&[u32]> {
    table
        .binary_search_by_key(&codepoint, |(key, _)| *key)
        .ok()
        .map(|index| table[index].1.as_slice())
}

fn lookup_u64(table: &[(u64, u32)], key: u64) -> Option<u32> {
    table
        .binary_search_by_key(&key, |(entry, _)| *entry)
        .ok()
        .map(|index| table[index].1)
}

pub fn is_alnum(codepoint: u32) -> bool {
    range_contains(&tables().alnum_ranges, codepoint)
}

pub fn is_space(codepoint: u32) -> bool {
    range_contains(&tables().space_ranges, codepoint)
}

pub fn decimal_digit(codepoint: u32) -> Option<u8> {
    lookup(&tables().decimal, codepoint)
}

fn range_contains(table: &[(u32, u32)], codepoint: u32) -> bool {
    table
        .binary_search_by(|(start, end)| {
            if codepoint < *start {
                std::cmp::Ordering::Greater
            } else if codepoint > *end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

pub fn casefold(codepoint: char) -> Vec<char> {
    lookup_values(&tables().casefold, codepoint as u32)
        .map(|values| values.iter().filter_map(|value| char::from_u32(*value)).collect())
        .unwrap_or_else(|| vec![codepoint])
}

pub fn casefold_str(value: &str) -> String {
    value.chars().flat_map(casefold).collect()
}

fn combining_class(codepoint: u32) -> u8 {
    lookup(&tables().ccc, codepoint).unwrap_or(0)
}

fn decompose(codepoint: u32, output: &mut Vec<u32>) {
    const SBASE: u32 = 0xAC00;
    const LBASE: u32 = 0x1100;
    const VBASE: u32 = 0x1161;
    const TBASE: u32 = 0x11A7;
    const LCOUNT: u32 = 19;
    const VCOUNT: u32 = 21;
    const TCOUNT: u32 = 28;
    const NCOUNT: u32 = VCOUNT * TCOUNT;
    const SCOUNT: u32 = LCOUNT * NCOUNT;
    if (SBASE..SBASE + SCOUNT).contains(&codepoint) {
        let index = codepoint - SBASE;
        output.push(LBASE + index / NCOUNT);
        output.push(VBASE + (index % NCOUNT) / TCOUNT);
        if index % TCOUNT != 0 {
            output.push(TBASE + index % TCOUNT);
        }
        return;
    }
    if let Some(parts) = lookup_values(&tables().decomp, codepoint) {
        for part in parts {
            decompose(*part, output);
        }
    } else {
        output.push(codepoint);
    }
}

fn compose_pair(left: u32, right: u32) -> Option<u32> {
    const SBASE: u32 = 0xAC00;
    const LBASE: u32 = 0x1100;
    const VBASE: u32 = 0x1161;
    const TBASE: u32 = 0x11A7;
    const LCOUNT: u32 = 19;
    const VCOUNT: u32 = 21;
    const TCOUNT: u32 = 28;
    const NCOUNT: u32 = VCOUNT * TCOUNT;
    const SCOUNT: u32 = LCOUNT * NCOUNT;
    if (LBASE..LBASE + LCOUNT).contains(&left) && (VBASE..VBASE + VCOUNT).contains(&right) {
        return Some(SBASE + (left - LBASE) * NCOUNT + (right - VBASE) * TCOUNT);
    }
    if (SBASE..SBASE + SCOUNT).contains(&left)
        && (left - SBASE) % TCOUNT == 0
        && (TBASE + 1..TBASE + TCOUNT).contains(&right)
    {
        return Some(left + right - TBASE);
    }
    let key = ((left as u64) << 21) | right as u64;
    lookup_u64(&tables().composition, key)
}

pub fn nfc(value: &str) -> String {
    let mut decomposed = Vec::new();
    for codepoint in value.chars() {
        decompose(codepoint as u32, &mut decomposed);
    }
    let mut ordered = Vec::with_capacity(decomposed.len());
    for codepoint in decomposed {
        let class = combining_class(codepoint);
        ordered.push(codepoint);
        if class != 0 {
            let mut index = ordered.len() - 1;
            while index > 0 {
                let previous = combining_class(ordered[index - 1]);
                if previous == 0 || previous <= class {
                    break;
                }
                ordered.swap(index - 1, index);
                index -= 1;
            }
        }
    }
    let mut composed = Vec::with_capacity(ordered.len());
    let mut starter = None;
    let mut last_class = 0u8;
    for codepoint in ordered {
        let class = combining_class(codepoint);
        if let Some(starter_index) = starter {
            if last_class < class || last_class == 0 {
                if let Some(value) = compose_pair(composed[starter_index], codepoint) {
                    composed[starter_index] = value;
                    continue;
                }
            }
        }
        if class == 0 {
            starter = Some(composed.len());
            last_class = 0;
        } else {
            last_class = class;
        }
        composed.push(codepoint);
    }
    composed.into_iter().filter_map(char::from_u32).collect()
}

fn html_lookup(value: &str) -> Option<&str> {
    tables()
        .html5
        .binary_search_by(|(key, _)| key.as_str().cmp(value))
        .ok()
        .map(|index| tables().html5[index].1.as_str())
}

fn invalid_codepoint(number: u32) -> bool {
    (1..=8).contains(&number)
        || number == 11
        || (14..=31).contains(&number)
        || (0x7F..=0x9F).contains(&number)
        || (0xFDD0..=0xFDEF).contains(&number)
        || (number & 0xFFFF) >= 0xFFFE
}

fn numeric_charref(number: u32) -> Option<char> {
    if let Some(value) = lookup(&tables().invalid_charrefs, number) {
        return char::from_u32(value);
    }
    if (0xD800..=0xDFFF).contains(&number) || number > 0x10FFFF {
        return Some('\u{FFFD}');
    }
    if invalid_codepoint(number) {
        return None;
    }
    char::from_u32(number).or(Some('\u{FFFD}'))
}

pub fn html_unescape(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut index = 0;
    let chars: Vec<(usize, char)> = value.char_indices().collect();
    while index < chars.len() {
        let (_byte_index, character) = chars[index];
        if character != '&' {
            output.push(character);
            index += 1;
            continue;
        }
        let start = index + 1;
        if start >= chars.len() {
            output.push('&');
            index += 1;
            continue;
        }
        let mut end = start;
        let mut numeric = false;
        let mut hexadecimal = false;
        if chars[start].1 == '#' {
            numeric = true;
            end += 1;
            if end < chars.len() && matches!(chars[end].1, 'x' | 'X') {
                hexadecimal = true;
                end += 1;
            }
        }
        if numeric {
            let digit_start = end;
            while end < chars.len()
                && if hexadecimal {
                    chars[end].1.is_ascii_hexdigit()
                } else {
                    chars[end].1.is_ascii_digit()
                }
            {
                end += 1;
            }
            if end > digit_start {
                let number = u32::from_str_radix(
                    &chars[digit_start..end]
                        .iter()
                        .map(|(_, c)| *c)
                        .collect::<String>(),
                    if hexadecimal { 16 } else { 10 },
                )
                .unwrap_or(0);
                if end < chars.len() && chars[end].1 == ';' {
                    end += 1;
                }
                if let Some(value) = numeric_charref(number) {
                    output.push(value);
                }
                index = end;
                continue;
            }
        }
        end = start;
        while end < chars.len()
            && !matches!(
                chars[end].1,
                '\t' | '\n' | '\u{000C}' | ' ' | '<' | '&' | '#' | ';'
            )
        {
            end += 1;
        }
        let has_semicolon = end < chars.len() && chars[end].1 == ';';
        let name_end = if has_semicolon { end + 1 } else { end };
        if name_end > start {
            let key: String = chars[start..name_end].iter().map(|(_, c)| *c).collect();
            if let Some(replacement) = html_lookup(&key) {
                output.push_str(replacement);
                index = name_end;
                continue;
            }
            let key_chars: Vec<char> = key.chars().collect();
            let mut matched_prefix = false;
            for prefix_len in (2..key_chars.len()).rev() {
                let prefix: String = key_chars[..prefix_len].iter().collect();
                if let Some(replacement) = html_lookup(&prefix) {
                    output.push_str(replacement);
                    output.extend(key_chars[prefix_len..].iter());
                    index = name_end;
                    matched_prefix = true;
                    break;
                }
            }
            if matched_prefix {
                continue;
            }
        }
        output.push('&');
        index = start;
    }
    output
}
