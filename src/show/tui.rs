use crate::error::Result;

use super::data::ShowData;

/// Launch the interactive TUI.
pub fn run_tui(data: ShowData) -> Result<()> {
    use crossterm::{
        event::{self, Event, KeyCode, KeyModifiers},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::prelude::*;

    let mut terminal = {
        enable_raw_mode().map_err(io_err)?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen).map_err(io_err)?;
        let backend = CrosstermBackend::new(stdout);
        Terminal::new(backend).map_err(io_err)?
    };

    let mut app = AppState::new(data);

    loop {
        terminal.draw(|f| views::render(f, &app)).map_err(io_err)?;

        if let Event::Key(key) = event::read().map_err(io_err)? {
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                _ => super::keymap::handle_key(&mut app, key),
            }
        }
    }

    // Restore terminal
    disable_raw_mode().map_err(io_err)?;
    execute!(std::io::stdout(), LeaveAlternateScreen).map_err(io_err)?;

    Ok(())
}

fn io_err(e: std::io::Error) -> crate::error::ChronicleError {
    crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    }
}

/// TUI application state.
pub struct AppState {
    pub data: ShowData,
    pub scroll_offset: usize,
    pub selected_region: Option<usize>,
    pub panel_expanded: bool,
    pub panel_scroll: usize,
    pub show_help: bool,
}

impl AppState {
    pub fn new(data: ShowData) -> Self {
        let initial_region = if data.regions.is_empty() {
            None
        } else {
            Some(0)
        };
        Self {
            data,
            scroll_offset: 0,
            selected_region: initial_region,
            panel_expanded: true,
            panel_scroll: 0,
            show_help: false,
        }
    }

    pub fn total_lines(&self) -> usize {
        self.data.source_lines.len()
    }

    /// Jump to the next annotated region.
    pub fn next_region(&mut self) {
        if self.data.regions.is_empty() {
            return;
        }
        let next = match self.selected_region {
            Some(i) if i + 1 < self.data.regions.len() => i + 1,
            _ => 0,
        };
        self.selected_region = Some(next);
        self.panel_scroll = 0;
        // Scroll to make the region visible
        let line = self.data.regions[next].region.lines.start as usize;
        if line > 0 {
            self.scroll_offset = line.saturating_sub(3);
        }
    }

    /// Jump to the previous annotated region.
    pub fn prev_region(&mut self) {
        if self.data.regions.is_empty() {
            return;
        }
        let prev = match self.selected_region {
            Some(0) | None => self.data.regions.len() - 1,
            Some(i) => i - 1,
        };
        self.selected_region = Some(prev);
        self.panel_scroll = 0;
        let line = self.data.regions[prev].region.lines.start as usize;
        if line > 0 {
            self.scroll_offset = line.saturating_sub(3);
        }
    }
}

mod views {
    use super::AppState;
    use ratatui::prelude::*;
    use ratatui::widgets::*;

    pub fn render(f: &mut Frame, app: &AppState) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // header
                Constraint::Min(1),    // main
                Constraint::Length(1), // status
            ])
            .split(f.area());

        render_header(f, app, chunks[0]);
        render_main(f, app, chunks[1]);
        render_status(f, app, chunks[2]);

        if app.show_help {
            render_help(f, f.area());
        }
    }

    fn render_header(f: &mut Frame, app: &AppState, area: Rect) {
        let commit_short = &app.data.commit[..7.min(app.data.commit.len())];
        let region_count = app.data.regions.len();
        let text = format!(
            " {} @ {} [{region_count} region{}]  [q]uit [n/N]ext/prev [Enter]expand [?]help",
            app.data.file_path,
            commit_short,
            if region_count == 1 { "" } else { "s" },
        );
        let header =
            Paragraph::new(text).style(Style::default().bg(Color::DarkGray).fg(Color::White));
        f.render_widget(header, area);
    }

    fn render_main(f: &mut Frame, app: &AppState, area: Rect) {
        if app.panel_expanded && app.selected_region.is_some() {
            // Split: source left, annotation right
            let panes = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                .split(area);

            render_source(f, app, panes[0]);
            render_annotation_panel(f, app, panes[1]);
        } else {
            render_source(f, app, area);
        }
    }

    fn render_source(f: &mut Frame, app: &AppState, area: Rect) {
        let visible_height = area.height as usize;
        let line_count = app.data.source_lines.len();
        let line_num_width = format!("{}", line_count).len();

        let mut lines: Vec<Line> = Vec::new();

        for i in app.scroll_offset..line_count.min(app.scroll_offset + visible_height) {
            let line_num = i + 1;
            let region_indices = app.data.annotation_map.regions_at_line(line_num as u32);

            // Gutter indicator
            let gutter = if !region_indices.is_empty() {
                // Check if this region is selected
                let is_selected = app
                    .selected_region
                    .is_some_and(|sel| region_indices.contains(&sel));
                if is_selected {
                    Span::styled("█ ", Style::default().fg(Color::Cyan))
                } else {
                    Span::styled("█ ", Style::default().fg(Color::DarkGray))
                }
            } else {
                Span::raw("  ")
            };

            let num = Span::styled(
                format!("{:>width$} ", line_num, width = line_num_width),
                Style::default().fg(Color::DarkGray),
            );

            let source_text = app
                .data
                .source_lines
                .get(i)
                .map(|s| s.as_str())
                .unwrap_or("");

            let source_span = Span::raw(source_text);

            lines.push(Line::from(vec![gutter, num, source_span]));
        }

        let source_widget = Paragraph::new(lines);
        f.render_widget(source_widget, area);
    }

    fn render_annotation_panel(f: &mut Frame, app: &AppState, area: Rect) {
        let region_idx = match app.selected_region {
            Some(i) => i,
            None => return,
        };
        let r = match app.data.regions.get(region_idx) {
            Some(r) => r,
            None => return,
        };

        let mut text_lines: Vec<Line> = Vec::new();

        // Anchor header
        text_lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", r.region.ast_anchor.unit_type),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                r.region.ast_anchor.name.clone(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        text_lines.push(Line::from(format!(
            "lines {}-{}",
            r.region.lines.start, r.region.lines.end,
        )));
        text_lines.push(Line::raw(""));

        // Intent
        text_lines.push(Line::styled(
            "Intent",
            Style::default().add_modifier(Modifier::BOLD),
        ));
        for wrapped in wrap_text(&r.region.intent, area.width.saturating_sub(2) as usize) {
            text_lines.push(Line::raw(format!("  {wrapped}")));
        }
        text_lines.push(Line::raw(""));

        // Reasoning
        if let Some(ref reasoning) = r.region.reasoning {
            text_lines.push(Line::styled(
                "Reasoning",
                Style::default().add_modifier(Modifier::BOLD),
            ));
            for wrapped in wrap_text(reasoning, area.width.saturating_sub(2) as usize) {
                text_lines.push(Line::raw(format!("  {wrapped}")));
            }
            text_lines.push(Line::raw(""));
        }

        // Constraints
        if !r.region.constraints.is_empty() {
            text_lines.push(Line::styled(
                "Constraints",
                Style::default().add_modifier(Modifier::BOLD),
            ));
            for c in &r.region.constraints {
                let source = match c.source {
                    crate::schema::v1::ConstraintSource::Author => "author",
                    crate::schema::v1::ConstraintSource::Inferred => "inferred",
                };
                text_lines.push(Line::raw(format!("  - {} [{source}]", c.text)));
            }
            text_lines.push(Line::raw(""));
        }

        // Dependencies
        if !r.region.semantic_dependencies.is_empty() {
            text_lines.push(Line::styled(
                "Dependencies",
                Style::default().add_modifier(Modifier::BOLD),
            ));
            for d in &r.region.semantic_dependencies {
                text_lines.push(Line::raw(format!("  -> {} :: {}", d.file, d.anchor)));
            }
            text_lines.push(Line::raw(""));
        }

        // Risk notes
        if let Some(ref risk) = r.region.risk_notes {
            text_lines.push(Line::styled(
                "Risk",
                Style::default().add_modifier(Modifier::BOLD).fg(Color::Red),
            ));
            for wrapped in wrap_text(risk, area.width.saturating_sub(2) as usize) {
                text_lines.push(Line::raw(format!("  {wrapped}")));
            }
            text_lines.push(Line::raw(""));
        }

        // Corrections
        if !r.region.corrections.is_empty() {
            text_lines.push(Line::styled(
                format!("Corrections ({})", r.region.corrections.len()),
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Yellow),
            ));
            text_lines.push(Line::raw(""));
        }

        // Metadata
        text_lines.push(Line::styled(
            "Metadata",
            Style::default().fg(Color::DarkGray),
        ));
        text_lines.push(Line::raw(format!(
            "  commit: {}",
            &r.commit[..7.min(r.commit.len())]
        )));
        text_lines.push(Line::raw(format!("  time: {}", r.timestamp)));
        if !r.region.tags.is_empty() {
            text_lines.push(Line::raw(format!("  tags: {}", r.region.tags.join(", "))));
        }

        // Apply scroll offset
        let scrolled: Vec<Line> = text_lines.into_iter().skip(app.panel_scroll).collect();

        let panel = Paragraph::new(scrolled).block(
            Block::default()
                .borders(Borders::LEFT)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        f.render_widget(panel, area);
    }

    fn render_status(f: &mut Frame, app: &AppState, area: Rect) {
        let status = if let Some(idx) = app.selected_region {
            let r = &app.data.regions[idx];
            format!(
                " region {}/{} │ {} │ lines {}-{} │ {} deps",
                idx + 1,
                app.data.regions.len(),
                r.region.ast_anchor.name,
                r.region.lines.start,
                r.region.lines.end,
                r.region.semantic_dependencies.len(),
            )
        } else {
            format!(
                " {} lines │ {} regions │ scroll: {}",
                app.data.source_lines.len(),
                app.data.regions.len(),
                app.scroll_offset + 1,
            )
        };

        let status_bar =
            Paragraph::new(status).style(Style::default().bg(Color::DarkGray).fg(Color::White));
        f.render_widget(status_bar, area);
    }

    fn render_help(f: &mut Frame, area: Rect) {
        let help_text = vec![
            Line::styled(
                "Keyboard Shortcuts",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Line::raw(""),
            Line::raw("  j/↓         Scroll down"),
            Line::raw("  k/↑         Scroll up"),
            Line::raw("  Ctrl-d/PgDn Half page down"),
            Line::raw("  Ctrl-u/PgUp Half page up"),
            Line::raw("  g/Home      Jump to top"),
            Line::raw("  G/End       Jump to bottom"),
            Line::raw("  n           Next annotated region"),
            Line::raw("  N           Previous annotated region"),
            Line::raw("  Enter       Toggle annotation panel"),
            Line::raw("  J/K         Scroll annotation panel"),
            Line::raw("  q           Quit"),
            Line::raw("  ?           Toggle this help"),
        ];

        let block = Block::default()
            .title(" Help ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let help_width = 50.min(area.width);
        let help_height = 16.min(area.height);
        let x = area.x + (area.width.saturating_sub(help_width)) / 2;
        let y = area.y + (area.height.saturating_sub(help_height)) / 2;
        let help_area = Rect::new(x, y, help_width, help_height);

        // Clear background
        f.render_widget(Clear, help_area);

        let help = Paragraph::new(help_text).block(block);
        f.render_widget(help, help_area);
    }

    /// Simple word-wrapping for annotation text.
    fn wrap_text(text: &str, width: usize) -> Vec<String> {
        if width == 0 {
            return vec![text.to_string()];
        }
        let mut lines = Vec::new();
        let mut current = String::new();
        for word in text.split_whitespace() {
            if current.len() + word.len() + 1 > width && !current.is_empty() {
                lines.push(current);
                current = word.to_string();
            } else {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(word);
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
        if lines.is_empty() {
            lines.push(String::new());
        }
        lines
    }
}
