//! Validate measured instruction counts before reporting a CI regression result.

use std::collections::BTreeSet;

#[derive(Debug, PartialEq, Eq)]
pub struct Report {
    pub measurements: usize,
    pub worst_basis_points: u64,
    pub added: Vec<String>,
}

struct Case {
    line: usize,
    name: Option<String>,
    current: u64,
    baseline: Option<u64>,
}

/// Every case in a base run must carry a nonzero count.
pub fn baseline(input: &str) -> Result<Report, String> {
    let cases = cases(input)?;
    if cases.is_empty() {
        return Err("instruction measurements missing".to_owned());
    }
    Ok(Report {
        measurements: cases.len(),
        worst_basis_points: 0,
        added: Vec::new(),
    })
}

/// A case missing from `base` is reported as added; a case `base` measured
/// must be paired with a nonzero count and stay within `max_percent`.
pub fn compare(input: &str, base: &str, max_percent: u32) -> Result<Report, String> {
    let measured: BTreeSet<String> = cases(base)?.into_iter().filter_map(|c| c.name).collect();
    let mut report = Report {
        measurements: 0,
        worst_basis_points: 0,
        added: Vec::new(),
    };
    for case in cases(input)? {
        let Some(baseline) = case.baseline else {
            match case.name {
                Some(name) if !measured.contains(&name) => {
                    report.added.push(name);
                    continue;
                }
                Some(name) => {
                    return Err(format!(
                        "line {}: {name} was measured on base but has no paired count",
                        case.line
                    ));
                }
                None => return Err(format!("line {}: baseline missing", case.line)),
            }
        };
        let current = case.current;
        if u128::from(current) * 100 > u128::from(baseline) * (100 + u128::from(max_percent)) {
            return Err(format!(
                "line {}: {current} instructions exceed {max_percent}% above {baseline}",
                case.line
            ));
        }
        let basis_points =
            u128::from(current.saturating_sub(baseline)) * 10_000 / u128::from(baseline);
        report.worst_basis_points = report
            .worst_basis_points
            .max(u64::try_from(basis_points).map_err(|_| "instruction ratio overflow".to_owned())?);
        report.measurements += 1;
    }
    if report.measurements == 0 {
        return Err("no instruction measurement is paired with the base".to_owned());
    }
    Ok(report)
}

/// A case is an unindented benchmark id line followed by its indented counters.
fn cases(input: &str) -> Result<Vec<Case>, String> {
    let text = strip_ansi(input);
    let mut cases = Vec::new();
    let mut name = None;
    for (index, line) in text.lines().enumerate() {
        let Some((_, counters)) = line.split_once("Instructions:") else {
            if !line.trim().is_empty()
                && !line.starts_with(char::is_whitespace)
                && !line.starts_with("Iai-Callgrind")
            {
                name = Some(line.trim().to_owned());
            }
            continue;
        };
        let (current, baseline) = counters
            .split_once('|')
            .map_or((counters, None), |(new, old)| (new, Some(old)));
        let baseline = match baseline.map(|old| old.split_whitespace().next().unwrap_or_default()) {
            None | Some("N/A") => None,
            Some(old) => Some(counter(old, "baseline", index + 1)?),
        };
        cases.push(Case {
            line: index + 1,
            name: name.take(),
            current: counter(current, "current", index + 1)?,
            baseline,
        });
    }
    Ok(cases)
}

fn counter(input: &str, role: &str, line: usize) -> Result<u64, String> {
    let token = input.split_whitespace().next().unwrap_or_default();
    let value = token.replace(',', "").parse::<u64>();
    match value {
        Ok(value) if value > 0 => Ok(value),
        _ => Err(format!("line {line}: {role} instructions must be nonzero")),
    }
}

fn strip_ansi(input: &str) -> String {
    let mut output = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for code in chars.by_ref() {
                if ('@'..='~').contains(&code) {
                    break;
                }
            }
        } else {
            output.push(ch);
        }
    }
    output
}
