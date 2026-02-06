use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::tui::AppState;

/// Map a key event to an app state mutation.
pub fn handle_key(app: &mut AppState, key: KeyEvent) {
    match key.code {
        // Scroll source
        KeyCode::Char('j') | KeyCode::Down => {
            if app.scroll_offset + 1 < app.total_lines() {
                app.scroll_offset += 1;
                update_selected_region_from_scroll(app);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.scroll_offset = app.scroll_offset.saturating_sub(1);
            update_selected_region_from_scroll(app);
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let half = 20; // approximate half screen
            app.scroll_offset = (app.scroll_offset + half).min(app.total_lines().saturating_sub(1));
            update_selected_region_from_scroll(app);
        }
        KeyCode::PageDown => {
            let half = 20;
            app.scroll_offset = (app.scroll_offset + half).min(app.total_lines().saturating_sub(1));
            update_selected_region_from_scroll(app);
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.scroll_offset = app.scroll_offset.saturating_sub(20);
            update_selected_region_from_scroll(app);
        }
        KeyCode::PageUp => {
            app.scroll_offset = app.scroll_offset.saturating_sub(20);
            update_selected_region_from_scroll(app);
        }
        KeyCode::Char('g') | KeyCode::Home => {
            app.scroll_offset = 0;
            update_selected_region_from_scroll(app);
        }
        KeyCode::Char('G') | KeyCode::End => {
            app.scroll_offset = app.total_lines().saturating_sub(1);
            update_selected_region_from_scroll(app);
        }

        // Region navigation
        KeyCode::Char('n') => app.next_region(),
        KeyCode::Char('N') => app.prev_region(),

        // Panel
        KeyCode::Enter => app.panel_expanded = !app.panel_expanded,
        KeyCode::Char('J') => app.panel_scroll += 1,
        KeyCode::Char('K') => app.panel_scroll = app.panel_scroll.saturating_sub(1),

        // Help
        KeyCode::Char('?') => app.show_help = !app.show_help,

        _ => {}
    }
}

/// When scrolling, auto-select the region at the current scroll position.
fn update_selected_region_from_scroll(app: &mut AppState) {
    let current_line = (app.scroll_offset + 1) as u32;
    let regions = app.data.annotation_map.regions_at_line(current_line);
    if let Some(&idx) = regions.first() {
        if app.selected_region != Some(idx) {
            app.selected_region = Some(idx);
            app.panel_scroll = 0;
        }
    }
}
