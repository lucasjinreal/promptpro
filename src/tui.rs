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
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Terminal,
};
use std::io;
use std::thread;
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

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
    show_delete_confirmation: bool,
    show_add_prompt_dialog: bool,
    new_prompt_key_input: String,
    input_cursor_pos: usize,
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
            show_delete_confirmation: false,
            show_add_prompt_dialog: false,
            new_prompt_key_input: String::new(),
            input_cursor_pos: 0,
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
            show_delete_confirmation: false,
            show_add_prompt_dialog: false,
            new_prompt_key_input: String::new(),
            input_cursor_pos: 0,
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
                    self.content = self
                        .vault
                        .get(key, VersionSelector::Version(version.version))?;
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
            match self
                .vault
                .update(key, &self.edit_content, Some("Updated via TUI".to_string()))
            {
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

    fn start_add_prompt(&mut self) {
        self.show_add_prompt_dialog = true;
        self.new_prompt_key_input.clear();
        self.input_cursor_pos = 0;
        self.message = "Enter prompt key name, then press Enter".to_string();
    }

    fn add_prompt(&mut self) -> Result<()> {
        if self.new_prompt_key_input.is_empty() {
            self.message = "Prompt key cannot be empty".to_string();
            return Ok(());
        }

        // Check if key already exists
        if self.keys.contains(&self.new_prompt_key_input) {
            self.message = format!("Key '{}' already exists", self.new_prompt_key_input);
            return Ok(());
        }

        // Create a temporary file for editing
        use std::fs;
        let temp_file = std::env::temp_dir().join(format!(
            "promptpro_new_{}.txt",
            self.new_prompt_key_input
                .replace("/", "_")
                .replace(" ", "_")
        ));

        // Create an empty file initially
        fs::write(&temp_file, "")?;

        // Get editor from environment or default to vim
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());

        // Open external editor
        let status = std::process::Command::new(&editor)
            .arg(&temp_file)
            .status()?;

        if status.success() {
            // Read the content from the temp file
            let content = fs::read_to_string(&temp_file)?;

            if !content.trim().is_empty() {
                // Add the prompt to the vault
                self.vault.add(&self.new_prompt_key_input, &content)?;
                self.message = format!("Added new prompt: '{}'", self.new_prompt_key_input);

                // Refresh the key list
                self.refresh_keys()?;
                // Select the new key
                if let Some(index) = self
                    .keys
                    .iter()
                    .position(|k| k == &self.new_prompt_key_input)
                {
                    self.selected_key_index = index;
                    self.refresh_versions()?;
                }
            } else {
                self.message = "Prompt content was empty, not saved".to_string();
            }
        } else {
            self.message = "Editor exited with error, prompt not saved".to_string();
        }

        // Clean up temp file
        let _ = fs::remove_file(&temp_file);

        // Exit dialog mode
        self.show_add_prompt_dialog = false;
        self.new_prompt_key_input.clear();
        self.input_cursor_pos = 0;

        Ok(())
    }

    fn cancel_add_prompt(&mut self) {
        self.show_add_prompt_dialog = false;
        self.new_prompt_key_input.clear();
        self.input_cursor_pos = 0;
        self.message = "Add prompt cancelled".to_string();
    }

    fn handle_input_char(&mut self, c: char) {
        if self.show_add_prompt_dialog {
            // Insert character at cursor position
            self.new_prompt_key_input.insert(self.input_cursor_pos, c);
            self.input_cursor_pos += 1;
        }
    }

    fn handle_backspace(&mut self) {
        if self.show_add_prompt_dialog && self.input_cursor_pos > 0 {
            self.new_prompt_key_input.remove(self.input_cursor_pos - 1);
            self.input_cursor_pos -= 1;
        }
    }

    fn handle_left_arrow(&mut self) {
        if self.show_add_prompt_dialog && self.input_cursor_pos > 0 {
            self.input_cursor_pos -= 1;
        }
    }

    fn handle_right_arrow(&mut self) {
        if self.show_add_prompt_dialog && self.input_cursor_pos < self.new_prompt_key_input.len() {
            self.input_cursor_pos += 1;
        }
    }

    fn delete_current_key(&mut self) -> Result<()> {
        if let Some(key) = self.keys.get(self.selected_key_index) {
            match self.vault.delete_prompt_key(key) {
                Ok(()) => {
                    self.message = format!("Deleted prompt key: '{}'", key);
                    self.refresh_keys()?;
                    // Reset indices if there are keys left
                    if !self.keys.is_empty() {
                        self.selected_key_index = self
                            .selected_key_index
                            .min(self.keys.len().saturating_sub(1));
                        self.refresh_versions()?;
                    } else {
                        self.selected_key_index = 0;
                        self.versions.clear();
                        self.selected_version_index = 0;
                        self.content.clear();
                    }
                }
                Err(e) => {
                    self.message = format!("Error deleting key '{}': {}", key, e);
                }
            }
        }
        Ok(())
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

async fn show_splash_screen<B: Backend>(terminal: &mut Terminal<B>) -> Result<()> {
    let ascii_art = vec![
        " ██████╗  ██████╗  ██████╗   ██████╗ ",
        " ██╔══██╗ ██╔══██╗ ██╔══██╗ ██╔═══██╗",
        " ██████╔╝ ██████╔╝ ██████╔╝ ██║   ██║",
        " ██╔═══╝  ██╔═══╝  ██╔══██╗ ██║   ██║",
        " ██║      ██║      ██║  ██║ ╚██████╔╝",
        " ╚═╝      ╚═╝      ╚═╝  ╚═╝  ╚═════╝ ",
    ];

    let mut counter = 0;
    let duration = 20;

    while counter < duration {
        terminal.draw(|f| {
            let size = f.size();
            let block = Block::default().style(Style::default().bg(Color::Black));
            f.render_widget(block, size);

            let ascii_height = ascii_art.len() as u16;
            let start_y = size.height.saturating_sub(ascii_height + 4) / 2;

            for (i, line) in ascii_art.iter().enumerate() {
                let y_pos = start_y + i as u16;

                // Gradient proportional
                let t = i as f32 / (ascii_height.saturating_sub(1) as f32).max(1.0);
                let color = Color::Rgb(50, (100.0 + 155.0 * t) as u8, (200.0 + 55.0 * t) as u8);

                // Correct display width for Unicode characters
                let line_width = line.width() as u16;
                let start_x = size.width.saturating_sub(line_width) / 2;

                let paragraph = Paragraph::new(line.to_string())
                    .style(Style::default().fg(color).add_modifier(Modifier::BOLD))
                    .alignment(Alignment::Left);

                let line_area = ratatui::layout::Rect {
                    x: start_x,
                    y: y_pos,
                    width: line_width,
                    height: 1,
                };

                f.render_widget(paragraph, line_area);
            }

            // Version text
            let version_text = format!("v{}", env!("CARGO_PKG_VERSION"));
            let version_x = size.width.saturating_sub(version_text.len() as u16) / 2;
            let version_y = start_y + ascii_height + 1;
            let version_area = ratatui::layout::Rect {
                x: version_x,
                y: version_y,
                width: version_text.len() as u16,
                height: 1,
            };
            f.render_widget(
                Paragraph::new(version_text).style(Style::default().fg(Color::DarkGray)),
                version_area,
            );

            // Loading indicator
            let loading_text = format!("Loading{}", ".".repeat((counter % 4) as usize));
            let loading_x = size.width.saturating_sub(loading_text.len() as u16) / 2;
            let loading_y = version_y + 1;
            let loading_area = ratatui::layout::Rect {
                x: loading_x,
                y: loading_y,
                width: loading_text.len() as u16,
                height: 1,
            };
            f.render_widget(
                Paragraph::new(loading_text).style(Style::default().fg(Color::Cyan)),
                loading_area,
            );
        })?;

        thread::sleep(Duration::from_millis(200));
        counter += 1;
    }

    Ok(())
}

pub async fn run() -> Result<()> {
    // For now, skip splash screen to ensure TUI works properly
    run_with_app(App::new()?).await
}

pub async fn run_with_key(key: String) -> Result<()> {
    // For the specific key case, we'll skip the splash screen for better UX
    run_with_app(App::new_with_key(key)?).await
}

async fn run_with_app(mut app: App) -> Result<()> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    show_splash_screen(&mut terminal).await?;
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
                            if app.show_add_prompt_dialog {
                                // Move cursor right in input field when in add prompt dialog
                                app.handle_right_arrow();
                            } else {
                                // Move to next panel
                                match app.active_panel {
                                    Panel::Keys => app.switch_panel(Panel::Versions),
                                    Panel::Versions => app.switch_panel(Panel::Content),
                                    Panel::Content => app.switch_panel(Panel::Tags),
                                    Panel::Tags => app.switch_panel(Panel::Keys), // Loop back
                                }
                            }
                        }
                        KeyCode::Left => {
                            if app.show_add_prompt_dialog {
                                // Move cursor left in input field when in add prompt dialog
                                app.handle_left_arrow();
                            } else {
                                // Move to previous panel
                                match app.active_panel {
                                    Panel::Tags => app.switch_panel(Panel::Content),
                                    Panel::Content => app.switch_panel(Panel::Versions),
                                    Panel::Versions => app.switch_panel(Panel::Keys),
                                    Panel::Keys => app.switch_panel(Panel::Tags), // Loop back
                                }
                            }
                        }
                        KeyCode::Enter => {
                            if app.show_add_prompt_dialog {
                                // Add the prompt with the entered key name
                                app.add_prompt()?;
                            } else {
                                // Apply or remove tag for the currently selected version
                                if app.active_panel == Panel::Tags && !app.versions.is_empty() {
                                    if let Some(tag) = app.selected_tag.clone() {
                                        if let Some(version) =
                                            app.versions.get(app.selected_version_index)
                                        {
                                            if let Some(key) = app.keys.get(app.selected_key_index)
                                            {
                                                // Check if the tag is currently on this version
                                                let is_currently_tagged =
                                                    version.tags.contains(&tag);

                                                if is_currently_tagged {
                                                    // Tag is currently on this version
                                                    // For dev tag, we don't allow removing from latest version
                                                    if tag == "dev"
                                                        && app.selected_version_index
                                                            == app.versions.len().saturating_sub(1)
                                                    {
                                                        // This is the latest version with dev tag - we can't remove it since dev should stay on latest
                                                        app.message = "Cannot remove 'dev' tag. It always points to the latest version.".to_string();
                                                    } else if tag == "dev" {
                                                        // This is not the latest version, but dev tag is on it somehow - user can't remove it
                                                        app.message = "Cannot modify 'dev' tag manually. It always points to the latest version.".to_string();
                                                    } else {
                                                        // For other tags, allow removal by tagging version 1 if available, otherwise find another version
                                                        let target_version = if app.versions.len()
                                                            > 1
                                                            && version.version != 1
                                                        {
                                                            1 // Move to version 1
                                                        } else if app.versions.len() > 1 {
                                                            // We're on version 1, move to version 2
                                                            2
                                                        } else {
                                                            // Only one version - clear the tag by applying it back to same version to force storage update
                                                            // Actually, let's just not allow removal if it's the only version
                                                            app.message = format!("Cannot remove tag '{}' from the only available version", tag);
                                                            return Ok(());
                                                        };

                                                        match app.vault.tag(
                                                            key,
                                                            &tag,
                                                            target_version,
                                                        ) {
                                                            Ok(_) => {
                                                                app.message = format!(
                                                                    "Moved tag '{}' to version {}",
                                                                    tag, target_version
                                                                );
                                                                app.refresh_versions()?;
                                                            }
                                                            Err(e) => {
                                                                app.message = format!(
                                                                    "Error moving tag: {}",
                                                                    e
                                                                );
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    // Tag is not on this version - apply it here
                                                    // First check if this version already has any tags applied
                                                    let version_already_has_tags =
                                                        !version.tags.is_empty();

                                                    if version_already_has_tags {
                                                        // This version already has tags, so first remove all tags from this version
                                                        // We'll apply the selected tag after removing existing ones
                                                        // For now, we'll just apply the tag - the backend will handle moving tags from other versions
                                                        match app.vault.tag(
                                                            key,
                                                            &tag,
                                                            version.version,
                                                        ) {
                                                            Ok(_) => {
                                                                app.message = format!("Applied tag '{}' to version {} (replacing previous tags)", tag, version.version);
                                                                app.refresh_versions()?;
                                                            }
                                                            Err(e) => {
                                                                app.message = format!(
                                                                    "Error applying tag: {}",
                                                                    e
                                                                );
                                                            }
                                                        }
                                                    } else {
                                                        // No existing tags on this version, just apply the new tag
                                                        match app.vault.tag(
                                                            key,
                                                            &tag,
                                                            version.version,
                                                        ) {
                                                            Ok(_) => {
                                                                app.message = format!("Applied tag '{}' to version {}", tag, version.version);
                                                                app.refresh_versions()?;
                                                            }
                                                            Err(e) => {
                                                                app.message = format!(
                                                                    "Error applying tag: {}",
                                                                    e
                                                                );
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Char('x') => {
                            // Apply or remove tag for the currently selected version (same as Enter for convenience)
                            if app.active_panel == Panel::Tags && !app.versions.is_empty() {
                                if let Some(tag) = app.selected_tag.clone() {
                                    if let Some(version) =
                                        app.versions.get(app.selected_version_index)
                                    {
                                        if let Some(key) = app.keys.get(app.selected_key_index) {
                                            // Check if the tag is currently on this version
                                            let is_currently_tagged = version.tags.contains(&tag);

                                            if is_currently_tagged {
                                                // Tag is currently on this version
                                                // For dev tag, we don't allow removing from latest version
                                                if tag == "dev"
                                                    && app.selected_version_index
                                                        == app.versions.len().saturating_sub(1)
                                                {
                                                    // This is the latest version with dev tag - we can't remove it since dev should stay on latest
                                                    app.message = "Cannot remove 'dev' tag. It always points to the latest version.".to_string();
                                                } else if tag == "dev" {
                                                    // This is not the latest version, but dev tag is on it somehow - user can't remove it
                                                    app.message = "Cannot modify 'dev' tag manually. It always points to the latest version.".to_string();
                                                } else {
                                                    // For other tags, allow removal by tagging version 1 if available, otherwise find another version
                                                    let target_version = if app.versions.len() > 1
                                                        && version.version != 1
                                                    {
                                                        1 // Move to version 1
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
                                                            app.message = format!(
                                                                "Moved tag '{}' to version {}",
                                                                tag, target_version
                                                            );
                                                            app.refresh_versions()?;
                                                        }
                                                        Err(e) => {
                                                            app.message =
                                                                format!("Error moving tag: {}", e);
                                                        }
                                                    }
                                                }
                                            } else {
                                                // Tag is not on this version - apply it here
                                                // First check if this version already has any tags applied
                                                let version_already_has_tags =
                                                    !version.tags.is_empty();

                                                if version_already_has_tags {
                                                    // This version already has tags, so first remove all tags from this version
                                                    // We'll apply the selected tag after removing existing ones
                                                    // For now, we'll just apply the tag - the backend will handle moving tags from other versions
                                                    match app.vault.tag(key, &tag, version.version)
                                                    {
                                                        Ok(_) => {
                                                            app.message = format!("Applied tag '{}' to version {} (replacing previous tags)", tag, version.version);
                                                            app.refresh_versions()?;
                                                        }
                                                        Err(e) => {
                                                            app.message = format!(
                                                                "Error applying tag: {}",
                                                                e
                                                            );
                                                        }
                                                    }
                                                } else {
                                                    // No existing tags on this version, just apply the new tag
                                                    match app.vault.tag(key, &tag, version.version)
                                                    {
                                                        Ok(_) => {
                                                            app.message = format!(
                                                                "Applied tag '{}' to version {}",
                                                                tag, version.version
                                                            );
                                                            app.refresh_versions()?;
                                                        }
                                                        Err(e) => {
                                                            app.message = format!(
                                                                "Error applying tag: {}",
                                                                e
                                                            );
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
                                        app.selected_key_index =
                                            (app.selected_key_index + 1) % app.keys.len();
                                        app.refresh_versions()?;
                                    }
                                }
                                Panel::Versions => {
                                    // Move down in version list
                                    if !app.versions.is_empty() {
                                        app.selected_version_index =
                                            (app.selected_version_index + 1) % app.versions.len();

                                        if let Some(version) =
                                            app.versions.get(app.selected_version_index)
                                        {
                                            if let Some(key) = app.keys.get(app.selected_key_index)
                                            {
                                                app.content = app.vault.get(
                                                    key,
                                                    VersionSelector::Version(version.version),
                                                )?;
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
                                        app.selected_key_index = app
                                            .selected_key_index
                                            .saturating_sub(1)
                                            .min(app.keys.len().saturating_sub(1));
                                        app.refresh_versions()?;
                                    }
                                }
                                Panel::Versions => {
                                    // Move up in version list
                                    if !app.versions.is_empty() {
                                        app.selected_version_index = app
                                            .selected_version_index
                                            .saturating_sub(1)
                                            .min(app.versions.len().saturating_sub(1));

                                        if let Some(version) =
                                            app.versions.get(app.selected_version_index)
                                        {
                                            if let Some(key) = app.keys.get(app.selected_key_index)
                                            {
                                                app.content = app.vault.get(
                                                    key,
                                                    VersionSelector::Version(version.version),
                                                )?;
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
                                if let Some(version) = app.versions.get(app.selected_version_index)
                                {
                                    if let Some(key) = app.keys.get(app.selected_key_index) {
                                        // Get content to edit
                                        let content_to_edit = app
                                            .vault
                                            .get(key, VersionSelector::Version(version.version))?;

                                        // Write content to a temporary file
                                        use std::fs;
                                        use std::process::Command;

                                        let temp_file =
                                            std::env::temp_dir().join("promptpro_edit.txt");
                                        fs::write(&temp_file, &content_to_edit)?;

                                        // Get editor from environment or default to vim
                                        let editor = std::env::var("EDITOR")
                                            .unwrap_or_else(|_| "vim".to_string());

                                        // Open external editor
                                        let status =
                                            Command::new(&editor).arg(&temp_file).status()?;

                                        // Read the updated content if the editor exited successfully
                                        if status.success() {
                                            let updated_content = fs::read_to_string(&temp_file)?;
                                            if updated_content != content_to_edit {
                                                // Update the vault with the new content
                                                app.vault.update(
                                                    key,
                                                    &updated_content,
                                                    Some("Updated via external editor".to_string()),
                                                )?;
                                                app.message =
                                                    format!("Updated content for '{}'", key);
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
                        KeyCode::Char('a')
                            if !app.show_add_prompt_dialog
                                && !app.show_delete_confirmation
                                && app.active_panel == Panel::Keys =>
                        {
                            // Start adding a new prompt (when on Keys panel)
                            app.start_add_prompt();
                        }
                        KeyCode::Char('d') => {
                            // Delete current key (when on Keys panel)
                            if app.active_panel == Panel::Keys {
                                // Confirm deletion with user before proceeding
                                if !app.keys.is_empty() {
                                    if let Some(_key) = app.keys.get(app.selected_key_index) {
                                        // Show confirmation dialog
                                        app.show_delete_confirmation = true;
                                    }
                                }
                            }
                        }
                        KeyCode::Char('y') if app.show_delete_confirmation => {
                            // Confirm deletion
                            if !app.keys.is_empty() {
                                if let Some(key) = app.keys.get(app.selected_key_index).cloned() {
                                    app.delete_current_key()?;
                                    app.show_delete_confirmation = false;
                                    app.message = format!("Deleted prompt key: '{}'", key);
                                }
                            }
                        }
                        KeyCode::Char('n') => {
                            // Handle 'n' key press differently based on context
                            if app.show_delete_confirmation {
                                // Cancel deletion if in confirmation mode
                                app.show_delete_confirmation = false;
                                app.message = "Deletion cancelled".to_string();
                            } else {
                                // Create new prompt when not in confirmation mode
                                app.message = "New prompt creation would happen here".to_string();
                            }
                        }
                        KeyCode::Esc if app.show_delete_confirmation => {
                            // Cancel deletion
                            app.show_delete_confirmation = false;
                            app.message = "Deletion cancelled".to_string();
                        }
                        _ => {}
                    },
                    Mode::Editing => match key.code {
                        KeyCode::Esc => {
                            // Cancel edit
                            app.mode = Mode::Normal;
                        }
                        KeyCode::Char('s')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
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
            Constraint::Min(1),    // Main content area for 4 panels
            Constraint::Length(3), // Footer for instructions
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
        .split(main_chunks[0]); // Split the main content area

    // Panel borders with active panel highlighting
    let keys_border_style = if matches!(app.active_panel, Panel::Keys) {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let versions_border_style = if matches!(app.active_panel, Panel::Versions) {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let content_border_style = if matches!(app.active_panel, Panel::Content) {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let tags_border_style = if matches!(app.active_panel, Panel::Tags) {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
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
                (
                    format!("> {}", key),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
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
                .style(keys_border_style),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(45, 45, 65))
                .add_modifier(Modifier::BOLD),
        );

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
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                // For multiple tags, we'll use the first significant tag for coloring
                // Or use a combination approach with priority
                if version.tags.contains(&"stable".to_string())
                    && version.tags.contains(&"release".to_string())
                {
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
                .style(versions_border_style),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(45, 45, 65))
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(version_list, chunks[1]);

    // Content Panel with Markdown-like styling
    let content_paragraph = match app.mode {
        Mode::Editing => Paragraph::new(app.edit_content.as_str())
            .block(
                Block::default()
                    .title(" Content (Editing) ")
                    .borders(Borders::ALL)
                    .style(content_border_style),
            )
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false })
            .scroll((0, 0)),
        _ => {
            // Simple markdown-like styling for content display
            let styled_content = app
                .content
                .lines()
                .map(|line| {
                    if line.starts_with("# ") {
                        // H1: Bold with Cyan
                        Line::from(vec![Span::styled(
                            line,
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        )])
                    } else if line.starts_with("## ") {
                        // H2: Bold with Blue
                        Line::from(vec![Span::styled(
                            line,
                            Style::default()
                                .fg(Color::Blue)
                                .add_modifier(Modifier::BOLD),
                        )])
                    } else if line.starts_with("**") && line.ends_with("**") {
                        // Bold text
                        Line::from(vec![Span::styled(
                            line.trim_matches('*'),
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        )])
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
                        .style(content_border_style),
                )
                .wrap(Wrap { trim: false })
                .scroll((0, 0))
        }
    };

    f.render_widget(content_paragraph, chunks[2]);

    // Tags Panel
    let tags = ["stable", "dev", "release"];
    let tag_items: Vec<ListItem> = tags
        .iter()
        .map(|tag_str| {
            let tag = tag_str.to_string();
            let is_selected = app.selected_tag.as_ref() == Some(&tag);
            // Check specifically if this tag is applied to the current version
            let is_currently_on_this_version = app
                .versions
                .get(app.selected_version_index)
                .map_or(false, |v| v.tags.contains(&tag));

            let (text, style) = if is_currently_on_this_version {
                // This specific tag is applied to the currently selected version
                if is_selected {
                    (
                        format!("> [x] {}", tag),
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    (format!("  [x] {}", tag), Style::default().fg(Color::Green))
                }
            } else {
                if is_selected {
                    (
                        format!("> [ ] {}", tag),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    (
                        format!("  [ ] {}", tag),
                        Style::default().fg(Color::DarkGray),
                    )
                }
            };

            ListItem::new(vec![Line::from(Span::styled(text, style))])
        })
        .collect();

    let tag_list = List::new(tag_items)
        .block(
            Block::default()
                .title(" Tags ")
                .borders(Borders::ALL)
                .style(tags_border_style),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(45, 45, 65))
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(tag_list, chunks[3]);

    // Check if we need to show add prompt dialog
    if app.show_add_prompt_dialog {
        // Create a centered popup window for adding a new prompt
        let popup_width = 60;
        let popup_height = 6;
        let area = f.size();
        let popup_x = (area.width - popup_width) / 2;
        let popup_y = (area.height - popup_height) / 2;
        let popup_area = ratatui::layout::Rect {
            x: popup_x,
            y: popup_y,
            width: popup_width,
            height: popup_height,
        };

        // Create the add prompt dialog
        let add_dialog_block = Block::default()
            .title(" Add New Prompt ")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Blue).fg(Color::White));

        let text_lines = vec![
            Line::from("Enter prompt key name:"),
            Line::from(""),
            Line::from(vec![Span::raw(&app.new_prompt_key_input)]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to edit in external editor, "),
                Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to cancel"),
            ]),
        ];

        let paragraph = Paragraph::new(text_lines)
            .block(add_dialog_block)
            .alignment(ratatui::layout::Alignment::Left)
            .wrap(Wrap { trim: false });

        f.render_widget(paragraph, popup_area);

        // Draw cursor for input field (only if cursor is within the terminal bounds)
        if app.input_cursor_pos <= app.new_prompt_key_input.len() {
            let cursor_x = popup_x
                + 1
                + "Enter prompt key name:".len() as u16
                + 1
                + app.input_cursor_pos as u16;
            let cursor_y = popup_y + 2; // Position of the input field line
                                        // Only set cursor if it's within terminal bounds to avoid errors
            if cursor_x < f.size().width && cursor_y < f.size().height {
                f.set_cursor(cursor_x, cursor_y);
            }
        }
    }
    // Check if we need to show delete confirmation popup
    else if app.show_delete_confirmation {
        if let Some(key) = app.keys.get(app.selected_key_index) {
            // Create a centered popup window for confirmation
            let popup_width = 50;
            let popup_height = 8;
            let area = f.size();
            let popup_x = (area.width - popup_width) / 2;
            let popup_y = (area.height - popup_height) / 2;
            let popup_area = ratatui::layout::Rect {
                x: popup_x,
                y: popup_y,
                width: popup_width,
                height: popup_height,
            };

            // Create the confirmation popup
            let delete_confirmation_block = Block::default()
                .title(" Confirm Deletion ")
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::Red).fg(Color::White));

            let text_lines = vec![
                Line::from(""),
                Line::from(vec![Span::styled(
                    format!("Delete '{}'?", key),
                    Style::default().add_modifier(Modifier::BOLD),
                )]),
                Line::from(""),
                Line::from("This action cannot be undone."),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Y", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" to confirm, "),
                    Span::styled("N", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" to cancel"),
                ]),
            ];

            let paragraph = Paragraph::new(text_lines)
                .block(delete_confirmation_block)
                .alignment(ratatui::layout::Alignment::Center)
                .wrap(Wrap { trim: false });

            f.render_widget(paragraph, popup_area);
        }
    }

    // Footer with instructions
    let footer_text = match app.mode {
        Mode::Normal => {
            let panel_desc = if app.show_delete_confirmation {
                "Confirm deletion: Y(es) / N(o) or Esc"
            } else if app.show_add_prompt_dialog {
                "Enter key name, then press Enter to edit in external editor"
            } else {
                match app.active_panel {
                    Panel::Keys => "Keys: j/k to navigate, d to delete, a to add",
                    Panel::Versions => "Versions: j/k to navigate",
                    Panel::Content => "Content: e to edit, o for external editor",
                    Panel::Tags => "Tags: j/k to select, Enter to apply",
                }
            };

            format!("←→: switch panels | {} | q: quit", panel_desc)
        }
        Mode::Editing => "Ctrl+S: save | Esc: cancel".to_string(),
    };

    let footer = Paragraph::new(format!("{} | {}", app.message, footer_text))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::DarkGray)),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(footer, main_chunks[1]); // Render footer in the bottom chunk
}
