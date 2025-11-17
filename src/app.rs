mod data;
mod file_directory;
mod thread_pool;
mod traits;
mod utils;

use data::TableColors;
use file_directory::FileDirectory;
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyModifiers},
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style, Stylize, palette::tailwind},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Cell, HighlightSpacing, Paragraph, Row, Table, TableState},
};
use std::{
    io::Result,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::sleep,
    time::{Duration, Instant},
};
pub use thread_pool::ThreadPool;
use traits::GetPhysicalSize;
use utils::format_bytes;

const INFO_TEXT: [&str; 2] = ["[Esc: exit] - [q: back/quit] - [Enter: open]", "[h: help]"];

pub struct App {
    table_state: TableState,
    table: Table<'static>,
    cache_directory: Arc<FileDirectory>,
    directory: Arc<FileDirectory>,
    thread_pool: Arc<ThreadPool>,
    scanning: bool,
    scanning_text: String,
    total_files: String,
    total_disk_usage: String,
    path_in_progress: String,
    event_poll: Arc<AtomicBool>,
    colors: TableColors,
    update_tick: Instant,
    dirty: bool,
    exit: bool,
}

impl App {
    pub fn new(thread_pool: Arc<ThreadPool>, directory: Arc<FileDirectory>) -> Self {
        Self {
            table_state: TableState::default(),
            table: Table::default(),
            colors: TableColors::new(),
            thread_pool,
            scanning: true,
            event_poll: Arc::new(AtomicBool::new(true)),
            scanning_text: String::from("Scanning"),
            cache_directory: Arc::clone(&directory),
            directory: Arc::clone(&directory),
            total_files: String::from(""),
            path_in_progress: String::from(""),
            total_disk_usage: String::from(""),
            update_tick: Instant::now(),
            dirty: true,
            exit: false,
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        sleep(Duration::from_millis(25));
        self.table_state.select(Some(0));

        while !self.exit {
            if self.scanning {
                if self.thread_pool.active_count.load(Ordering::Relaxed) == 0 {
                    self.scanning = false;
                    let event_poll = Arc::clone(&self.event_poll);
                    self.thread_pool.execute(move || {
                        sleep(Duration::from_secs(1));
                        event_poll.store(false, Ordering::Relaxed);
                        Ok(())
                    });
                }

                if self.update_tick.elapsed().as_millis() > 150 {
                    self.dirty = true;
                    self.update_tick = Instant::now();
                }
            } else {
                self.dirty = true;
            }

            terminal.draw(|frame| self.draw(frame))?;
            self.dirty = false;
            self.handle_events()?;
        }
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        let vertical = &Layout::vertical([
            Constraint::Max(3),
            Constraint::Max(3),
            Constraint::Min(3),
            Constraint::Max(1),
            Constraint::Length(4),
        ])
        .vertical_margin(1)
        .horizontal_margin(2);
        let rects = vertical.split(frame.area().clone());

        self.render_total(frame, rects[0]);
        self.render_header(frame, rects[1]);

        self.render_table(frame, rects[2]);

        if let Some(i) = self.table_state.selected() {
            let text = self.directory.entries.lock().unwrap()[i].name.clone();
            let p = Paragraph::new(format!(" Selected: [{text}]"))
                .fg(tailwind::WHITE)
                .bold();
            frame.render_widget(p, rects[3]);
        }

        self.render_footer(frame, rects[4]);
    }

    fn handle_events(&mut self) -> Result<()> {
        if !self.event_poll.load(Ordering::Relaxed) || event::poll(Duration::from_millis(150))? {
            match event::read()? {
                Event::Mouse(mouse) => match mouse.kind {
                    event::MouseEventKind::ScrollDown => self.next_row(),
                    event::MouseEventKind::ScrollUp => self.previous_row(),
                    _ => {}
                },
                Event::Key(key) => match key.code {
                    KeyCode::Char('q') => self.back(),
                    KeyCode::Char('h') => self.exit(),
                    KeyCode::Enter => self.open_selected_dir(),
                    KeyCode::Char('o') => self.open_selected_dir(),
                    KeyCode::Down | KeyCode::Char('j') => self.next_row(),
                    KeyCode::Up | KeyCode::Char('k') => self.previous_row(),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.exit();
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        Ok(())
    }

    fn exit(&mut self) {
        self.exit = true;
    }

    fn back(&mut self) {
        let current_dir = Arc::clone(&self.directory);

        if let Some(parent) = current_dir.parent.lock().unwrap().upgrade() {
            self.directory = Arc::clone(&parent);
            self.dirty = true;

            let idx = self
                .directory
                .entries
                .lock()
                .unwrap()
                .iter()
                .position(|a| Arc::ptr_eq(a, &current_dir));

            self.table_state.select(idx);
        } else {
            self.exit();
        }
    }

    fn next_row(&mut self) {
        if let Some(selected) = self.table_state.selected() {
            if selected < self.directory.entries.lock().unwrap().len() - 1 {
                self.table_state.select_next();
            }
        }
    }

    fn previous_row(&mut self) {
        if let Some(selected) = self.table_state.selected() {
            if selected > 0 {
                self.table_state.select_previous();
            }
        }
    }

    fn open_selected_dir(&mut self) {
        let selected = self.table_state.selected();

        if let Some(i) = selected {
            let current_dir = Arc::clone(&self.directory);
            let entry = Arc::clone(&current_dir.entries.lock().unwrap()[i]);

            if entry.is_dir {
                self.directory = entry;
                self.dirty = true;
                self.table_state.select_first();
            }
        }
    }

    fn render_total(&mut self, frame: &mut Frame, area: Rect) {
        let horizontal = &Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]);
        let rects = horizontal.split(area.clone());

        if self.dirty {
            self.total_files = self
                .thread_pool
                .total_files
                .load(Ordering::Relaxed)
                .to_string();
            self.total_disk_usage = utils::format_bytes(self.cache_directory.actual_size_bytes());
        }

        let text = Line::from(vec![
            Span::from("Total Scanned Files: "),
            Span::from(&self.total_files),
        ]);
        let paragraph = Paragraph::new(text).bold();
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(self.colors.header_bg));

        frame.render_widget(paragraph.block(block), rects[0]);

        let text = Line::from(vec![
            Span::from("Total Disk Usage: "),
            Span::from(&self.total_disk_usage),
        ]);
        let paragraph = Paragraph::new(text).bold();
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(self.colors.header_bg));

        frame.render_widget(paragraph.block(block), rects[1]);
    }

    fn render_header(&mut self, frame: &mut Frame, area: Rect) {
        if !self.scanning {
            self.scanning_text = String::from("Scanning Done");
            self.path_in_progress = self.directory.path.to_string_lossy().into_owned();
        } else if self.dirty {
            if self.scanning_text.len() > 13 {
                self.scanning_text.truncate(8);
            } else {
                self.scanning_text.push('.');
            }
            self.path_in_progress = self.thread_pool.path_in_progress.lock().unwrap().clone();
        }

        let paragraph = Paragraph::new(Text::from(self.path_in_progress.clone()));
        let block = Block::bordered()
            .border_style(Style::new().fg(self.colors.header_bg))
            .title(self.scanning_text.clone());

        frame.render_widget(paragraph.block(block), area);
    }

    fn render_table(&mut self, frame: &mut Frame, area: Rect) {
        if self.dirty {
            let header_style = Style::default()
                .fg(self.colors.header_fg)
                .bold()
                .bg(self.colors.header_bg);

            let selected_row_style = Style::default()
                .bg(self.colors.selected_row_style_bg)
                .bold()
                .fg(self.colors.selected_row_style_fg);

            let selected_col_style = Style::default().fg(self.colors.selected_column_style_fg);

            let selected_cell_style = Style::default()
                .add_modifier(Modifier::REVERSED)
                .fg(self.colors.selected_cell_style_fg);

            let entries_len = self.directory.entries.lock().unwrap().len();
            let total_size = format_bytes(self.directory.actual_size_bytes.load(Ordering::Relaxed));

            let header = [
                vec![Line::from(format!(" Name ({entries_len})"))],
                vec![Line::from(format!("| Disk_Usage ({total_size})"))],
                vec![Line::from("| Type")],
            ]
            .into_iter()
            .map(Cell::from)
            .collect::<Row>()
            .style(header_style)
            .top_margin(0)
            .bottom_margin(0)
            .height(1);

            let data = Arc::clone(&self.directory);
            {
                data.sort_entries_by_size_desc();
            }
            let entries = data.entries.lock().unwrap();

            let entries = entries.iter().map(|entry| {
                let item = entry.array();
                item.into_iter()
                    .enumerate()
                    .map(|(i, content)| {
                        if i == 0 {
                            let text = Text::from(vec![Line::from(format!(" {content}"))]);
                            Cell::from(text)
                        } else {
                            let text = Text::from(vec![Line::from(format!("| {content}"))]);
                            Cell::from(text)
                        }
                    })
                    .collect::<Row>()
                    .height(1)
            });

            let block = Block::bordered().border_style(Style::new().fg(self.colors.header_bg));

            self.table = Table::new(
                entries,
                [
                    Constraint::Fill(1),
                    Constraint::Fill(1),
                    Constraint::Max(14),
                ],
            )
            .header(header)
            .block(block)
            .row_highlight_style(selected_row_style)
            .column_highlight_style(selected_col_style)
            .cell_highlight_style(selected_cell_style)
            .highlight_spacing(HighlightSpacing::Always);
        }

        frame.render_stateful_widget(&self.table, area, &mut self.table_state);
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let info_footer = Paragraph::new(Text::from_iter(INFO_TEXT))
            .style(Style::new().fg(self.colors.row_fg))
            .centered()
            .block(
                Block::bordered()
                    .border_type(BorderType::Double)
                    .border_style(Style::new().fg(self.colors.header_bg)),
            );

        frame.render_widget(info_footer, area);
    }
}
