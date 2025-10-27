use crate::storage::PromptVault;
use crate::types::{VersionMeta, VersionSelector};
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Terminal,
};
use std::io;

#[derive(Clone)]
pub struct App {
    vault: PromptVault,
    keys: Vec<String>,
    selected_key_index: usize,
    versions: Vec<VersionMeta>,
    selected_version_index: usize,
    content: String,
    edit_content: String,
    mode: Mode,
    message: String,
    active_panel: Panel,
    show_tag_popup: bool,
    selected_tag: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Panel {
    Keys,
    Versions,
    Content,
    Tags,
}

#[derive(Clone)]
enum Mode {
    Normal,
    Editing,
}

impl App {
    fn new() -> Result<Self> {
        let vault = PromptVault::open_default()?;
        let keys = get_all_keys(&vault)?;
        let mut versions = Vec::new();
        let mut content = String::new();
        
        if let Some(first_key) = keys.first() {
            versions = vault.history(first_key)?;
            if let Some(latest_version) = versions.last() {
                content = vault.get(first_key, VersionSelector::Version(latest_version.version))?;
            }
        }

        Ok(App {
            vault,
            keys: keys.clone(),
            selected_key_index: 0,
            versions: versions.clone(),
            selected_version_index: versions.len().saturating_sub(1), // Select latest by default
            content,
            edit_content: String::new(),
            mode: Mode::Normal,
            message: String::new(),
            active_panel: Panel::Keys,
            show_tag_popup: false,
            selected_tag: None,
        })
    }

    fn new_with_key(key: String) -> Result<Self> {
        let vault = PromptVault::open_default()?;
        let keys = get_all_keys(&vault)?;
        let mut versions = Vec::new();
        let mut content = String::new();
        
        // Set the selected key to the provided key
        let selected_key_index = keys.iter().position(|k| k == &key).unwrap_or(0);
        
        versions = vault.history(&key)?;
        if let Some(latest_version) = versions.last() {
            content = vault.get(&key, VersionSelector::Version(latest_version.version))?;
        }

        Ok(App {
            vault,
            keys: keys.clone(),
            selected_key_index,
            versions: versions.clone(),
            selected_version_index: versions.len().saturating_sub(1), // Select latest by default
            content,
            edit_content: String::new(),
            mode: Mode::Normal,
            message: String::new(),
            active_panel: Panel::Keys,
            show_tag_popup: false,
            selected_tag: None,
        })
    }

    fn refresh_keys(&mut self) -> Result<()> {
        self.keys = get_all_keys(&self.vault)?;
        Ok(())
    }

    fn refresh_versions(&mut self) -> Result<()> {
        if let Some(key) = self.keys.get(self.selected_key_index) {
            self.versions = self.vault.history(key)?;
            // Make sure we select the latest version if possible
            if !self.versions.is_empty() {
                self.selected_version_index = self.versions.len().saturating_sub(1);
                
                if let Some(version) = self.versions.get(self.selected_version_index) {
                    self.content = self.vault.get(key, VersionSelector::Version(version.version))?;
                }
            } else {
                self.selected_version_index = 0;
                self.content = String::new();
            }
        }
        Ok(())
    }

    fn save_content(&mut self) -> Result<()> {
        if let Some(key) = self.keys.get(self.selected_key_index) {
            match self.vault.update(key, &self.edit_content, Some("Updated via TUI".to_string())) {
                Ok(_) => {
                    self.message = format!("Saved changes to '{}'", key);
                    self.refresh_versions()?;
                }
                Err(e) => {
                    self.message = format!("Error saving: {}", e);
                }
            }
        }
        Ok(())
    }

    fn add_tag(&mut self, tag: &str) -> Result<()> {
        if let Some(key) = self.keys.get(self.selected_key_index) {
            if let Some(version) = self.versions.get(self.selected_version_index) {
                match self.vault.tag(key, tag, version.version) {
                    Ok(_) => {
                        self.message = format!("Tagged version {} as '{}'", version.version, tag);
                        self.refresh_versions()?;
                    }
                    Err(e) => {
                        self.message = format!("Error tagging: {}", e);
                    }
                }
            }
        }
        Ok(())
    }

    fn switch_panel(&mut self, panel: Panel) {
        self.active_panel = panel;
    }
}

fn get_all_keys(vault: &PromptVault) -> Result<Vec<String>> {
    let mut keys = std::collections::HashSet::new();
    
    // Scan through all version entries to extract unique keys
    for result in vault.db().scan_prefix(b"version:") {
        let (key, _) = result?;
        let key_str = String::from_utf8(key.to_vec())?;
        
        // Extract the key from the format "version:{key}:{version}"
        if let Some(stripped) = key_str.strip_prefix("version:") {
            if let Some(key_part) = stripped.split(':').next() {
                keys.insert(key_part.to_string());
            }
        }
    }
    
    let mut keys_vec: Vec<String> = keys.into_iter().collect();
    keys_vec.sort();
    Ok(keys_vec)
}

pub async fn run() -> Result<()> {
    run_with_app(App::new()?).await
}

pub async fn run_with_key(key: String) -> Result<()> {
    run_with_app(App::new_with_key(key)?).await
}

async fn run_with_app(mut app: App) -> Result<()> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let res = run_app(&mut terminal, &mut app);

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match app.mode.clone() {
                    Mode::Normal => match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Char('e') => {
                            // Enter edit mode
                            if app.active_panel == Panel::Content {
                                app.edit_content = app.content.clone();
                                app.mode = Mode::Editing;
                            }
                        }
                        KeyCode::Right => {
                            // Move to next panel
                            match app.active_panel {
                                Panel::Keys => app.switch_panel(Panel::Versions),
                                Panel::Versions => app.switch_panel(Panel::Content),
                                Panel::Content => app.switch_panel(Panel::Tags),
                                Panel::Tags => app.switch_panel(Panel::Keys), // Loop back
                            }
                        }
                        KeyCode::Left => {
                            // Move to previous panel
                            match app.active_panel {
                                Panel::Tags => app.switch_panel(Panel::Content),
                                Panel::Content => app.switch_panel(Panel::Versions),
                                Panel::Versions => app.switch_panel(Panel::Keys),
                                Panel::Keys => app.switch_panel(Panel::Tags), // Loop back
                            }
                        }
                        KeyCode::Enter | KeyCode::Char('x') => {
                            // Apply or remove tag for the currently selected version
                            if app.active_panel == Panel::Tags && !app.versions.is_empty() {
                                if let Some(tag) = app.selected_tag.clone() {
                                    if let Some(version) = app.versions.get(app.selected_version_index) {
                                        if let Some(key) = app.keys.get(app.selected_key_index) {
                                            // Check if the tag is currently on this version
                                            let is_currently_tagged = version.tags.contains(&tag);
                                            
                                            if is_currently_tagged {
                                                // Tag is currently on this version
                                                // For dev tag, we don't allow removing from latest version
                                                if tag == "dev" && app.selected_version_index == app.versions.len().saturating_sub(1) {
                                                    // This is the latest version with dev tag - we can't remove it since dev should stay on latest
                                                    app.message = "Cannot remove 'dev' tag. It always points to the latest version.".to_string();
                                                } else if tag == "dev" {
                                                    // This is not the latest version, but dev tag is on it somehow - user can't remove it
                                                    app.message = "Cannot modify 'dev' tag manually. It always points to the latest version.".to_string();
                                                } else {
                                                    // For other tags, allow removal by tagging version 1 if available, otherwise find another version
                                                    let target_version = if app.versions.len() > 1 && version.version != 1 {
                                                        1  // Move to version 1
                                                    } else if app.versions.len() > 1 {
                                                        // We're on version 1, move to version 2
                                                        2
                                                    } else {
                                                        // Only one version - clear the tag by applying it back to same version to force storage update
                                                        // Actually, let's just not allow removal if it's the only version
                                                        app.message = format!("Cannot remove tag '{}' from the only available version", tag);
                                                        return Ok(());
                                                    };
                                                    
                                                    match app.vault.tag(key, &tag, target_version) {
                                                        Ok(_) => {
                                                            app.message = format!("Moved tag '{}' to version {}", tag, target_version);
                                                            app.refresh_versions()?;
                                                        }
                                                        Err(e) => {
                                                            app.message = format!("Error moving tag: {}", e);
                                                        }
                                                    }
                                                }
                                            } else {
                                                // Tag is not on this version - apply it here
                                                // First check if this version already has any tags applied
                                                let version_already_has_tags = !version.tags.is_empty();
                                                
                                                if version_already_has_tags {
                                                    // This version already has tags, so first remove all tags from this version
                                                    // We'll apply the selected tag after removing existing ones
                                                    // For now, we'll just apply the tag - the backend will handle moving tags from other versions
                                                    match app.vault.tag(key, &tag, version.version) {
                                                        Ok(_) => {
                                                            app.message = format!("Applied tag '{}' to version {} (replacing previous tags)", tag, version.version);
                                                            app.refresh_versions()?;
                                                        }
                                                        Err(e) => {
                                                            app.message = format!("Error applying tag: {}", e);
                                                        }
                                                    }
                                                } else {
                                                    // No existing tags on this version, just apply the new tag
                                                    match app.vault.tag(key, &tag, version.version) {
                                                        Ok(_) => {
                                                            app.message = format!("Applied tag '{}' to version {}", tag, version.version);
                                                            app.refresh_versions()?;
                                                        }
                                                        Err(e) => {
                                                            app.message = format!("Error applying tag: {}", e);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            match app.active_panel {
                                Panel::Keys => {
                                    // Move down in key list
                                    if !app.keys.is_empty() {
                                        app.selected_key_index = (app.selected_key_index + 1) % app.keys.len();
                                        app.refresh_versions()?;
                                    }
                                }
                                Panel::Versions => {
                                    // Move down in version list
                                    if !app.versions.is_empty() {
                                        app.selected_version_index = (app.selected_version_index + 1) % app.versions.len();
                                        
                                        if let Some(version) = app.versions.get(app.selected_version_index) {
                                            if let Some(key) = app.keys.get(app.selected_key_index) {
                                                app.content = app.vault.get(key, VersionSelector::Version(version.version))?;
                                            }
                                        }
                                    }
                                }
                                Panel::Tags => {
                                    // Move down in tag selection
                                    let tags = ["stable", "dev", "release"];
                                    if app.selected_tag.is_none() {
                                        app.selected_tag = Some(tags[0].to_string());
                                    } else {
                                        let current = app.selected_tag.as_ref().unwrap();
                                        for (i, tag) in tags.iter().enumerate() {
                                            if tag == current {
                                                let next_idx = (i + 1) % tags.len();
                                                app.selected_tag = Some(tags[next_idx].to_string());
                                                break;
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            match app.active_panel {
                                Panel::Keys => {
                                    // Move up in key list
                                    if !app.keys.is_empty() {
                                        app.selected_key_index = app.selected_key_index
                                            .saturating_sub(1)
                                            .min(app.keys.len().saturating_sub(1));
                                        app.refresh_versions()?;
                                    }
                                }
                                Panel::Versions => {
                                    // Move up in version list
                                    if !app.versions.is_empty() {
                                        app.selected_version_index = app.selected_version_index
                                            .saturating_sub(1)
                                            .min(app.versions.len().saturating_sub(1));
                                        
                                        if let Some(version) = app.versions.get(app.selected_version_index) {
                                            if let Some(key) = app.keys.get(app.selected_key_index) {
                                                app.content = app.vault.get(key, VersionSelector::Version(version.version))?;
                                            }
                                        }
                                    }
                                }
                                Panel::Tags => {
                                    // Move up in tag selection
                                    let tags = ["stable", "dev", "release"];
                                    if app.selected_tag.is_none() {
                                        app.selected_tag = Some(tags[tags.len() - 1].to_string());
                                    } else {
                                        let current = app.selected_tag.as_ref().unwrap();
                                        for (i, tag) in tags.iter().enumerate() {
                                            if tag == current {
                                                let prev_idx = (i + tags.len() - 1) % tags.len();
                                                app.selected_tag = Some(tags[prev_idx].to_string());
                                                break;
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        KeyCode::Char('o') => {
                            // Open external editor when on content panel
                            if app.active_panel == Panel::Content && !app.versions.is_empty() {
                                if let Some(version) = app.versions.get(app.selected_version_index) {
                                    if let Some(key) = app.keys.get(app.selected_key_index) {
                                        // Get content to edit
                                        let content_to_edit = app.vault.get(key, VersionSelector::Version(version.version))?;
                                        
                                        // Write content to a temporary file
                                        use std::fs;
                                        use std::process::Command;
                                        
                                        let temp_file = std::env::temp_dir().join("promptpro_edit.txt");
                                        fs::write(&temp_file, &content_to_edit)?;
                                        
                                        // Get editor from environment or default to vim
                                        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
                                        
                                        // Open external editor
                                        let status = Command::new(&editor)
                                            .arg(&temp_file)
                                            .status()?;
                                        
                                        // Read the updated content if the editor exited successfully
                                        if status.success() {
                                            let updated_content = fs::read_to_string(&temp_file)?;
                                            if updated_content != content_to_edit {
                                                // Update the vault with the new content
                                                app.vault.update(key, &updated_content, Some("Updated via external editor".to_string()))?;
                                                app.message = format!("Updated content for '{}'", key);
                                                app.refresh_versions()?; // Refresh to get the new version
                                            } else {
                                                app.message = "No changes detected".to_string();
                                            }
                                        }
                                        
                                        // Clean up temp file
                                        let _ = fs::remove_file(&temp_file);
                                    }
                                }
                            }
                        }
                        KeyCode::Char('n') => {
                            // Create new prompt
                            app.message = "New prompt creation would happen here".to_string();
                        }
                        _ => {}
                    },
                    Mode::Editing => match key.code {
                        KeyCode::Esc => {
                            // Cancel edit
                            app.mode = Mode::Normal;
                        }
                        KeyCode::Char('s') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                            // Save content
                            app.save_content()?;
                            app.mode = Mode::Normal;
                        }
                        _ => {}
                    },
                }
            }
        }
    }
}

fn ui(f: &mut ratatui::Frame, app: &App) {
    // Main layout: split between content area and footer
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),      // Main content area for 4 panels
            Constraint::Length(3),   // Footer for instructions
        ])
        .split(f.size());
    
    // 4-column layout for the main content area
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(30),
            Constraint::Percentage(20),
        ])
        .split(main_chunks[0]);  // Split the main content area

    // Panel borders with active panel highlighting
    let keys_border_style = if matches!(app.active_panel, Panel::Keys) {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    
    let versions_border_style = if matches!(app.active_panel, Panel::Versions) {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    
    let content_border_style = if matches!(app.active_panel, Panel::Content) {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    
    let tags_border_style = if matches!(app.active_panel, Panel::Tags) {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    // Keys List Panel
    let key_items: Vec<ListItem> = app
        .keys
        .iter()
        .enumerate()
        .map(|(i, key)| {
            let is_selected = i == app.selected_key_index;
            let (text, style) = if is_selected {
                (format!("> {}", key), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            } else {
                (format!("  {}", key), Style::default().fg(Color::White))
            };
            ListItem::new(vec![Line::from(Span::styled(text, style))])
        })
        .collect();
    
    let key_list = List::new(key_items)
        .block(
            Block::default()
                .title(" Keys ")
                .borders(Borders::ALL)
                .style(keys_border_style)
        )
        .highlight_style(Style::default().bg(Color::Rgb(45, 45, 65)).add_modifier(Modifier::BOLD));
    
    f.render_widget(key_list, chunks[0]);

    // Versions List Panel
    let version_items: Vec<ListItem> = app
        .versions
        .iter()
        .enumerate()
        .map(|(i, version)| {
            let is_selected = i == app.selected_version_index;
            let tags_str = if version.tags.is_empty() {
                "".to_string()
            } else {
                format!(" [{}]", version.tags.join(","))
            };
            let text = format!(
                "{} v{}{} ({})",
                if is_selected { ">" } else { " " },
                version.version,
                tags_str,
                version.timestamp.format("%m-%d %H:%M")
            );
            let style = if is_selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                // For multiple tags, we'll use the first significant tag for coloring
                // Or use a combination approach with priority
                if version.tags.contains(&"stable".to_string()) && version.tags.contains(&"release".to_string()) {
                    // If both stable and release, use a special color
                    Style::default().fg(Color::Rgb(255, 165, 0)) // Orange
                } else if version.tags.contains(&"stable".to_string()) {
                    Style::default().fg(Color::Green)
                } else if version.tags.contains(&"dev".to_string()) {
                    Style::default().fg(Color::Blue)
                } else if version.tags.contains(&"release".to_string()) {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::White)
                }
            };
            ListItem::new(vec![Line::from(Span::styled(text, style))])
        })
        .collect();
    
    let version_list = List::new(version_items)
        .block(
            Block::default()
                .title(" Versions ")
                .borders(Borders::ALL)
                .style(versions_border_style)
        )
        .highlight_style(Style::default().bg(Color::Rgb(45, 45, 65)).add_modifier(Modifier::BOLD));
    
    f.render_widget(version_list, chunks[1]);

    // Content Panel with Markdown-like styling
    let content_paragraph = match app.mode {
        Mode::Editing => {
            Paragraph::new(app.edit_content.as_str())
                .block(
                    Block::default()
                        .title(" Content (Editing) ")
                        .borders(Borders::ALL)
                        .style(content_border_style)
                )
                .style(Style::default().fg(Color::White))
                .wrap(Wrap { trim: false })
                .scroll((0, 0))
        }
        _ => {
            // Simple markdown-like styling for content display
            let styled_content = app.content
                .lines()
                .map(|line| {
                    if line.starts_with("# ") {
                        // H1: Bold with Cyan
                        Line::from(vec![Span::styled(line, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))])
                    } else if line.starts_with("## ") {
                        // H2: Bold with Blue
                        Line::from(vec![Span::styled(line, Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD))])
                    } else if line.starts_with("**") && line.ends_with("**") {
                        // Bold text
                        Line::from(vec![Span::styled(line.trim_matches('*'), Style::default().fg(Color::White).add_modifier(Modifier::BOLD))])
                    } else if line.starts_with("* ") || line.starts_with("- ") {
                        // List items
                        Line::from(vec![Span::styled(line, Style::default().fg(Color::Yellow))])
                    } else {
                        // Regular text
                        Line::from(vec![Span::styled(line, Style::default().fg(Color::White))])
                    }
                })
                .collect::<Vec<Line>>();
            
            Paragraph::new(styled_content)
                .block(
                    Block::default()
                        .title(" Content ")
                        .borders(Borders::ALL)
                        .style(content_border_style)
                )
                .wrap(Wrap { trim: false })
                .scroll((0, 0))
        }
    };
    
    f.render_widget(content_paragraph, chunks[2]);

    // Tags Panel
    let tags = ["stable", "dev", "release"];
    let tag_items: Vec<ListItem> = tags.iter().map(|tag_str| {
        let tag = tag_str.to_string();
        let is_selected = app.selected_tag.as_ref() == Some(&tag);
        // Check specifically if this tag is applied to the current version
        let is_currently_on_this_version = app.versions
            .get(app.selected_version_index)
            .map_or(false, |v| v.tags.contains(&tag));
        
        let (text, style) = if is_currently_on_this_version {
            // This specific tag is applied to the currently selected version
            if is_selected {
                (format!("> [x] {}", tag), 
                 Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
            } else {
                (format!("  [x] {}", tag), 
                 Style::default().fg(Color::Green))
            }
        } else {
            if is_selected {
                (format!("> [ ] {}", tag), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            } else {
                (format!("  [ ] {}", tag), Style::default().fg(Color::DarkGray))
            }
        };
        
        ListItem::new(vec![Line::from(Span::styled(text, style))])
    }).collect();
    
    let tag_list = List::new(tag_items)
        .block(
            Block::default()
                .title(" Tags ")
                .borders(Borders::ALL)
                .style(tags_border_style)
        )
        .highlight_style(Style::default().bg(Color::Rgb(45, 45, 65)).add_modifier(Modifier::BOLD));
    
    f.render_widget(tag_list, chunks[3]);

    // Footer with instructions
    let footer_text = match app.mode {
        Mode::Normal => {
            let panel_desc = match app.active_panel {
                Panel::Keys => "Keys: j/k to navigate",
                Panel::Versions => "Versions: j/k to navigate",
                Panel::Content => "Content: e to edit, o for external editor",
                Panel::Tags => "Tags: j/k to select, Enter to apply",
            };
            
            format!("←→: switch panels | {} | q: quit", panel_desc)
        },
        Mode::Editing => "Ctrl+S: save | Esc: cancel".to_string(),
    };
    
    let footer = Paragraph::new(format!("{} | {}", app.message, footer_text))
        .block(Block::default().borders(Borders::ALL).style(Style::default().bg(Color::DarkGray)))
        .style(Style::default().fg(Color::White));
    
    f.render_widget(footer, main_chunks[1]);  // Render footer in the bottom chunk
}