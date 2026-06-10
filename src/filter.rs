use crate::sampler::ProcessRow;

#[derive(Debug, Default)]
pub struct Filter {
    terms: Vec<Term>,
}

impl Filter {
    pub fn parse(raw: &str) -> Self {
        let terms = raw.split_whitespace().map(Term::parse).collect();
        Self { terms }
    }

    pub fn matches(&self, process: &ProcessRow) -> bool {
        self.terms.iter().all(|term| term.matches(process))
    }
}

#[derive(Debug)]
enum Term {
    Text(String),
    Field { field: TextField, needle: String },
    Numeric { field: NumField, op: Op, value: f64 },
}

impl Term {
    fn parse(raw: &str) -> Self {
        if let Some(term) = parse_numeric(raw) {
            return term;
        }
        if let Some((field, needle)) = raw.split_once(':')
            && let Some(field) = TextField::parse(field)
        {
            return Term::Field {
                field,
                needle: needle.to_lowercase(),
            };
        }
        Term::Text(raw.to_lowercase())
    }

    fn matches(&self, process: &ProcessRow) -> bool {
        match self {
            Term::Text(needle) => process.search_text.contains(needle.as_str()),
            Term::Field { field, needle } => contains_ignore_case(field.value(process), needle),
            Term::Numeric { field, op, value } => op.compare(field.actual(process), *value),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum TextField {
    User,
    Name,
    Status,
    Command,
    Pid,
}

impl TextField {
    fn parse(field: &str) -> Option<Self> {
        match field.to_lowercase().as_str() {
            "user" => Some(Self::User),
            "name" => Some(Self::Name),
            "status" | "state" => Some(Self::Status),
            "cmd" | "command" => Some(Self::Command),
            "pid" => Some(Self::Pid),
            _ => None,
        }
    }

    fn value(self, process: &ProcessRow) -> &str {
        match self {
            Self::Name => &process.sort_name,
            Self::User => &process.user,
            Self::Status => &process.status,
            Self::Command => &process.command,
            Self::Pid => &process.pid_str,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum NumField {
    Cpu,
    Mem,
    Pid,
}

impl NumField {
    fn parse(field: &str) -> Option<Self> {
        match field.to_lowercase().as_str() {
            "cpu" => Some(Self::Cpu),
            "mem" | "memory" | "rss" => Some(Self::Mem),
            "pid" => Some(Self::Pid),
            _ => None,
        }
    }

    fn parse_value(self, raw: &str) -> Option<f64> {
        match self {
            Self::Cpu | Self::Pid => raw.trim().parse().ok(),
            Self::Mem => parse_size(raw),
        }
    }

    fn actual(self, process: &ProcessRow) -> f64 {
        match self {
            Self::Cpu => process.cpu_usage as f64,
            Self::Mem => process.memory as f64,
            Self::Pid => process.pid as f64,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Op {
    Gt,
    Lt,
    Ge,
    Le,
}

impl Op {
    fn compare(self, actual: f64, expected: f64) -> bool {
        match self {
            Self::Gt => actual > expected,
            Self::Lt => actual < expected,
            Self::Ge => actual >= expected,
            Self::Le => actual <= expected,
        }
    }
}

fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    let needle_lower = needle.to_lowercase();
    haystack.to_lowercase().contains(&needle_lower)
}

fn parse_numeric(raw: &str) -> Option<Term> {
    const OPERATORS: [(&str, Op); 4] =
        [(">=", Op::Ge), ("<=", Op::Le), (">", Op::Gt), ("<", Op::Lt)];
    for (token, op) in OPERATORS {
        let Some(index) = raw.find(token) else {
            continue;
        };
        let field = &raw[..index];
        let value = &raw[index + token.len()..];
        if field.is_empty() || value.is_empty() {
            return None;
        }
        let field = NumField::parse(field)?;
        let value = field.parse_value(value)?;
        return Some(Term::Numeric { field, op, value });
    }
    None
}

fn parse_size(raw: &str) -> Option<f64> {
    let raw = raw.trim().to_lowercase();
    const SUFFIXES: [(&str, f64); 14] = [
        ("tib", 1099511627776.0),
        ("tb", 1e12),
        ("gib", 1073741824.0),
        ("gb", 1e9),
        ("mib", 1048576.0),
        ("mb", 1e6),
        ("kib", 1024.0),
        ("kb", 1e3),
        ("t", 1e12),
        ("g", 1e9),
        ("m", 1e6),
        ("k", 1e3),
        ("b", 1.0),
        ("", 1.0),
    ];
    for (suffix, multiplier) in SUFFIXES {
        let Some(prefix) = raw.strip_suffix(suffix) else {
            continue;
        };
        let prefix = prefix.trim();
        if prefix.is_empty() {
            continue;
        }
        if let Ok(number) = prefix.parse::<f64>() {
            return Some(number * multiplier);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::sampler::{ProcessRow, ProcessTrend};

    use super::Filter;

    fn process(pid: u32, name: &str, user: &str, cpu: f32, memory: u64) -> ProcessRow {
        let status = "running";
        let user_str = user.to_string();
        let cmd_str = format!("/usr/bin/{name}");
        ProcessRow {
            pid,
            pid_str: pid.to_string(),
            parent_pid: None,
            name: name.to_string(),
            sort_name: name.to_lowercase(),
            user: user_str.clone(),
            command: cmd_str.clone(),
            exe: "-".into(),
            cwd: "-".into(),
            status: status.into(),
            cpu_usage: cpu,
            memory,
            virtual_memory: memory,
            memory_percent: 0.0,
            disk_read_rate: 0.0,
            disk_write_rate: 0.0,
            total_disk_read: 0,
            total_disk_write: 0,
            network_in_rate: None,
            network_out_rate: None,
            total_network_in: None,
            total_network_out: None,
            network_attribution_supported: false,
            run_time: 0,
            start_time: 0,
            energy_impact: 0.0,
            trend: ProcessTrend::default(),
            selected_details: None,
            search_text: format!(
                "{pid} {} {} /usr/bin/{name} {status}",
                name.to_lowercase(),
                user.to_lowercase()
            ),
        }
    }

    #[test]
    fn plain_substring_still_matches() {
        let filter = Filter::parse("node");
        assert!(filter.matches(&process(1, "node", "milo", 1.0, 10)));
        assert!(!filter.matches(&process(2, "redis", "milo", 1.0, 10)));
    }

    #[test]
    fn numeric_predicates_compare_fields() {
        let busy = Filter::parse("cpu>50");
        assert!(busy.matches(&process(1, "node", "milo", 75.0, 10)));
        assert!(!busy.matches(&process(2, "node", "milo", 12.0, 10)));

        let big = Filter::parse("mem>=100mb");
        assert!(big.matches(&process(1, "node", "milo", 1.0, 200_000_000)));
        assert!(!big.matches(&process(2, "node", "milo", 1.0, 50_000_000)));
    }

    #[test]
    fn field_predicates_scope_the_match() {
        let mine = Filter::parse("user:milo");
        assert!(mine.matches(&process(1, "node", "milo", 1.0, 10)));
        assert!(!mine.matches(&process(2, "node", "root", 1.0, 10)));
    }

    #[test]
    fn terms_are_anded_together() {
        let filter = Filter::parse("cpu>50 user:milo");
        assert!(filter.matches(&process(1, "node", "milo", 80.0, 10)));
        assert!(!filter.matches(&process(2, "node", "milo", 5.0, 10)));
        assert!(!filter.matches(&process(3, "node", "root", 80.0, 10)));
    }

    #[test]
    fn unparseable_predicate_falls_back_to_substring() {
        let filter = Filter::parse("cpu");
        assert!(filter.matches(&process(1, "cpuminer", "milo", 1.0, 10)));
        assert!(!filter.matches(&process(2, "node", "milo", 1.0, 10)));
    }

    #[test]
    fn empty_query_matches_everything() {
        let filter = Filter::parse("   ");
        assert!(filter.matches(&process(1, "node", "milo", 1.0, 10)));
        assert!(filter.matches(&process(2, "redis", "root", 99.0, 9_000)));
    }
}
