//! Terminal styling plus a small aligned-table writer.

use chrono::{DateTime, Local, Utc};

// ------------------------------------------------------------------ colour

pub const FOCUS: &str = "1;36";
pub const WIN: &str = "32";
pub const LOSS: &str = "31";
pub const LIVE: &str = "33";
pub const DIM: &str = "2";

#[derive(Clone, Copy)]
pub struct Style {
    pub enabled: bool,
}

impl Style {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    fn paint(&self, code: &str, s: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn bold(&self, s: &str) -> String {
        self.paint("1", s)
    }
    pub fn dim(&self, s: &str) -> String {
        self.paint("2", s)
    }
    /// Bold cyan: the tracked team, wherever it shows up.
    pub fn focus(&self, s: &str) -> String {
        self.paint(FOCUS, s)
    }

    pub fn heading(&self, s: &str) {
        println!("\n{}", self.bold(s));
    }
}

// ------------------------------------------------------------------- table

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Right,
    Center,
}

/// A table cell: plain text plus an optional SGR code of its own.
pub struct Cell {
    text: String,
    code: Option<&'static str>,
}

impl Cell {
    pub fn styled(text: impl Into<String>, code: &'static str) -> Self {
        Self {
            text: text.into(),
            code: Some(code),
        }
    }
}

impl From<String> for Cell {
    fn from(text: String) -> Self {
        Self { text, code: None }
    }
}

impl From<&str> for Cell {
    fn from(text: &str) -> Self {
        Self {
            text: text.to_string(),
            code: None,
        }
    }
}

pub struct Table {
    headers: Vec<String>,
    aligns: Vec<Align>,
    rows: Vec<(Vec<Cell>, bool)>,
}

impl Table {
    pub fn new(headers: &[&str], aligns: &[Align]) -> Self {
        Self {
            headers: headers.iter().map(|h| h.to_string()).collect(),
            aligns: aligns.to_vec(),
            rows: Vec::new(),
        }
    }

    /// `highlight` marks the row as belonging to the tracked team.
    pub fn row<C: Into<Cell>>(&mut self, cells: Vec<C>, highlight: bool) {
        self.rows
            .push((cells.into_iter().map(Into::into).collect(), highlight));
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Widths come from the plain text; colour is applied per cell *after*
    /// padding, so a cell's own code never truncates the row's highlight.
    pub fn print(&self, style: &Style) {
        let cols = self.headers.len();
        let mut widths: Vec<usize> = self.headers.iter().map(|h| width(h)).collect();
        for (cells, _) in &self.rows {
            for (i, cell) in cells.iter().take(cols).enumerate() {
                widths[i] = widths[i].max(width(&cell.text));
            }
        }

        let header: String = (0..cols)
            .map(|i| pad(&self.headers[i], widths[i], Align::Left))
            .collect::<Vec<_>>()
            .join("  ");
        println!("{}", style.dim(header.trim_end()));

        for (cells, highlight) in &self.rows {
            // Trailing empty columns would otherwise print as coloured padding.
            let last = (0..cols)
                .rev()
                .find(|i| cells.get(*i).is_some_and(|c| !c.text.is_empty()))
                .unwrap_or(0);

            let line: String = (0..=last)
                .map(|i| {
                    let empty = Cell::from("");
                    let cell = cells.get(i).unwrap_or(&empty);
                    let align = self.aligns.get(i).copied().unwrap_or(Align::Left);
                    let mut padded = pad(&cell.text, widths[i], align);
                    if i == last {
                        padded = padded.trim_end().to_string();
                    }
                    // A focused row outranks dimming, but keeps W/L and LIVE colours.
                    let code = match (highlight, cell.code) {
                        _ if cell.text.is_empty() => None,
                        (true, None | Some(DIM)) => Some(FOCUS),
                        (_, code) => code,
                    };
                    match code {
                        Some(code) => style.paint(code, &padded),
                        None => padded,
                    }
                })
                .collect::<Vec<_>>()
                .join("  ");
            println!("{line}");
        }
    }
}

fn width(s: &str) -> usize {
    s.chars().count()
}

fn pad(s: &str, w: usize, align: Align) -> String {
    let len = width(s);
    if len >= w {
        return s.to_string();
    }
    let gap = w - len;
    match align {
        Align::Left => format!("{s}{}", " ".repeat(gap)),
        Align::Right => format!("{}{s}", " ".repeat(gap)),
        Align::Center => {
            let left = gap / 2;
            format!("{}{s}{}", " ".repeat(left), " ".repeat(gap - left))
        }
    }
}

pub fn truncate(s: &str, max: usize) -> String {
    if width(s) <= max {
        s.to_string()
    } else {
        let kept: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{kept}…")
    }
}

// -------------------------------------------------------------------- time

/// UTC instant rendered in the machine's local timezone.
pub fn local_time(t: DateTime<Utc>) -> String {
    t.with_timezone(&Local).format("%a %d %b %H:%M").to_string()
}

/// "in 2h 15m" / "3d ago" / "now".
pub fn relative(t: DateTime<Utc>) -> String {
    relative_to(t, Utc::now())
}

fn relative_to(t: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let delta = t.signed_duration_since(now);
    let future = delta.num_seconds() >= 0;
    let secs = delta.num_seconds().abs();

    let amount = if secs < 60 {
        return "now".to_string();
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        let (h, m) = (secs / 3600, (secs % 3600) / 60);
        if m == 0 {
            format!("{h}h")
        } else {
            format!("{h}h {m}m")
        }
    } else {
        let (d, h) = (secs / 86_400, (secs % 86_400) / 3600);
        if h == 0 {
            format!("{d}d")
        } else {
            format!("{d}d {h}h")
        }
    };

    if future {
        format!("in {amount}")
    } else {
        format!("{amount} ago")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn padding_respects_alignment() {
        assert_eq!(pad("T1", 5, Align::Left), "T1   ");
        assert_eq!(pad("T1", 5, Align::Right), "   T1");
        assert_eq!(pad("T1", 6, Align::Center), "  T1  ");
        assert_eq!(pad("overflow", 3, Align::Left), "overflow");
    }

    #[test]
    fn truncate_adds_an_ellipsis_only_when_needed() {
        assert_eq!(truncate("Gen.G", 10), "Gen.G");
        assert_eq!(truncate("Hanwha Life Esports", 8), "Hanwha …");
    }

    #[test]
    fn relative_reads_in_both_directions() {
        let now = Utc::now();
        assert_eq!(relative_to(now, now), "now");
        assert_eq!(relative_to(now + Duration::minutes(30), now), "in 30m");
        assert_eq!(
            relative_to(now + Duration::hours(2) + Duration::minutes(1), now),
            "in 2h 1m"
        );
        assert_eq!(relative_to(now + Duration::hours(3), now), "in 3h");
        assert_eq!(relative_to(now - Duration::days(3), now), "3d ago");
        assert_eq!(
            relative_to(now - Duration::days(1) - Duration::hours(5), now),
            "1d 5h ago"
        );
    }

    #[test]
    fn styling_is_a_no_op_when_colour_is_off() {
        assert_eq!(Style::new(false).focus("T1"), "T1");
        assert_eq!(Style::new(true).focus("T1"), "\x1b[1;36mT1\x1b[0m");
    }
}
