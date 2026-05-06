use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui_core::{backend::Backend, terminal::Terminal};
use ratatui_widgets::table::TableState;
use sysinfo::Signal;

use crate::{
    error::Result,
    sampler::{ProcessRow, Sampler, Snapshot},
    ui,
};

const NOTICE_TTL: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Cpu,
    Memory,
    Energy,
    Disk,
    Network,
}

impl Tab {
    pub const ALL: [Self; 5] = [
        Self::Cpu,
        Self::Memory,
        Self::Energy,
        Self::Disk,
        Self::Network,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Memory => "Memory",
            Self::Energy => "Energy",
            Self::Disk => "Disk",
            Self::Network => "Network",
        }
    }

    fn default_sort(self) -> SortKey {
        match self {
            Self::Cpu => SortKey::Cpu,
            Self::Memory => SortKey::Memory,
            Self::Energy => SortKey::Energy,
            Self::Disk => SortKey::DiskWrite,
            Self::Network => SortKey::Name,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Cpu,
    Memory,
    Energy,
    DiskRead,
    DiskWrite,
    Name,
    Pid,
    User,
    Runtime,
}

impl SortKey {
    pub fn title(self) -> &'static str {
        match self {
            Self::Cpu => "% CPU",
            Self::Memory => "Memory",
            Self::Energy => "Impact",
            Self::DiskRead => "Read/s",
            Self::DiskWrite => "Write/s",
            Self::Name => "Name",
            Self::Pid => "PID",
            Self::User => "User",
            Self::Runtime => "Runtime",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Notice {
    text: String,
    created_at: Instant,
}

impl Notice {
    fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            created_at: Instant::now(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    fn expired(&self) -> bool {
        self.remaining().is_zero()
    }

    fn remaining(&self) -> Duration {
        NOTICE_TTL.saturating_sub(self.created_at.elapsed())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum KillIntent {
    Term,
    Kill,
}

impl KillIntent {
    fn signal(self) -> Signal {
        match self {
            Self::Term => Signal::Term,
            Self::Kill => Signal::Kill,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Term => "TERM",
            Self::Kill => "KILL",
        }
    }
}

pub struct App {
    sampler: Sampler,
    snapshot: Snapshot,
    pub table_state: TableState,
    pub visible: Vec<usize>,
    pub tab: Tab,
    pub sort_key: SortKey,
    pub sort_desc: bool,
    pub filter: String,
    pub filter_mode: bool,
    pub show_details: bool,
    pub show_help: bool,
    pub notice: Option<Notice>,
    pub confirm: Option<KillIntent>,
    interval: Duration,
    last_refresh: Instant,
    should_quit: bool,
}

impl App {
    pub fn new(interval: Duration, initial_filter: Option<String>) -> Result<Self> {
        let mut sampler = Sampler::new()?;
        let snapshot = sampler.sample(None);
        let mut app = Self {
            sampler,
            snapshot,
            table_state: TableState::default(),
            visible: Vec::new(),
            tab: Tab::Cpu,
            sort_key: SortKey::Cpu,
            sort_desc: true,
            filter: initial_filter.unwrap_or_default(),
            filter_mode: false,
            show_details: true,
            show_help: false,
            notice: None,
            confirm: None,
            interval,
            last_refresh: Instant::now(),
            should_quit: false,
        };
        app.rebuild_view(None);
        Ok(app)
    }

    pub fn run<B>(&mut self, terminal: &mut Terminal<B>) -> Result<()>
    where
        B: Backend,
        B::Error: std::error::Error + Send + Sync + 'static,
    {
        let mut needs_draw = true;
        while !self.should_quit {
            if self.clear_expired_notice() {
                needs_draw = true;
            }

            if needs_draw {
                terminal.draw(|frame| ui::draw(frame, self))?;
                needs_draw = false;
            }

            let timeout = self.next_poll_timeout();
            if event::poll(timeout)? {
                needs_draw |= self.handle_event(event::read()?)?;
            }

            if self.last_refresh.elapsed() >= self.interval {
                self.refresh();
                needs_draw = true;
            }
        }
        Ok(())
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    pub fn selected_process(&self) -> Option<&ProcessRow> {
        self.table_state
            .selected()
            .and_then(|selected| self.visible.get(selected))
            .and_then(|index| self.snapshot.processes.get(*index))
    }

    pub fn selected_pid(&self) -> Option<u32> {
        self.selected_process().map(|process| process.pid)
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    fn handle_event(&mut self, event: Event) -> Result<bool> {
        let Event::Key(key) = event else {
            return Ok(false);
        };
        if key.kind != KeyEventKind::Press {
            return Ok(false);
        }

        if self.show_help {
            return Ok(self.handle_help_key(key));
        }
        if let Some(intent) = self.confirm {
            return self.handle_confirm_key(key, intent);
        }
        if self.filter_mode {
            return Ok(self.handle_filter_key(key));
        }

        let changed = match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
                true
            }
            KeyCode::Char('?') => {
                self.show_help = true;
                true
            }
            KeyCode::Char('/') => {
                self.filter_mode = true;
                true
            }
            KeyCode::Char('i') | KeyCode::Enter => {
                self.show_details = !self.show_details;
                true
            }
            KeyCode::Char('r') => {
                self.refresh();
                true
            }
            KeyCode::Char('s') => {
                self.cycle_sort();
                true
            }
            KeyCode::Char('S') => {
                self.sort_desc = !self.sort_desc;
                self.rebuild_view(self.selected_pid());
                true
            }
            KeyCode::Char('c') => {
                self.set_sort(SortKey::Cpu, true);
                true
            }
            KeyCode::Char('m') => {
                self.set_sort(SortKey::Memory, true);
                true
            }
            KeyCode::Char('e') => {
                self.set_sort(SortKey::Energy, true);
                true
            }
            KeyCode::Char('d') => {
                self.set_sort(SortKey::DiskWrite, true);
                true
            }
            KeyCode::Char('n') => {
                self.set_sort(SortKey::Name, false);
                true
            }
            KeyCode::Char('p') => {
                self.set_sort(SortKey::Pid, false);
                true
            }
            KeyCode::Char('u') => {
                self.set_sort(SortKey::User, false);
                true
            }
            KeyCode::Char('t') => self.begin_kill(KillIntent::Term),
            KeyCode::Char('f') => self.begin_kill(KillIntent::Kill),
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.adjust_interval(false);
                true
            }
            KeyCode::Char('-') => {
                self.adjust_interval(true);
                true
            }
            KeyCode::Char('1') => self.set_tab(Tab::Cpu),
            KeyCode::Char('2') => self.set_tab(Tab::Memory),
            KeyCode::Char('3') => self.set_tab(Tab::Energy),
            KeyCode::Char('4') => self.set_tab(Tab::Disk),
            KeyCode::Char('5') => self.set_tab(Tab::Network),
            KeyCode::Tab => self.next_tab(),
            KeyCode::BackTab => self.previous_tab(),
            KeyCode::Down | KeyCode::Char('j') => self.select_next(1),
            KeyCode::Up | KeyCode::Char('k') => self.select_previous(1),
            KeyCode::PageDown => self.select_next(10),
            KeyCode::PageUp => self.select_previous(10),
            KeyCode::Home => self.select_first(),
            KeyCode::End => self.select_last(),
            _ => false,
        };
        Ok(changed)
    }

    fn handle_help_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?') | KeyCode::Char('q') => {
                self.show_help = false;
                true
            }
            _ => false,
        }
    }

    fn handle_confirm_key(&mut self, key: KeyEvent, intent: KillIntent) -> Result<bool> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(pid) = self.selected_pid() {
                    match self.sampler.send_signal(pid, intent.signal()) {
                        Ok(()) => {
                            self.notice = Some(Notice::new(format!(
                                "sent {} to pid {}",
                                intent.label(),
                                pid
                            )));
                            self.refresh();
                        }
                        Err(error) => self.notice = Some(Notice::new(error.to_string())),
                    }
                }
                self.confirm = None;
                Ok(true)
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.confirm = None;
                self.notice = Some(Notice::new("process signal cancelled"));
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn handle_filter_key(&mut self, key: KeyEvent) -> bool {
        let previous_pid = self.selected_pid();
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                self.filter_mode = false;
                true
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.rebuild_view(previous_pid);
                true
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter.clear();
                self.rebuild_view(previous_pid);
                true
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter.push(c);
                self.rebuild_view(previous_pid);
                true
            }
            _ => false,
        }
    }

    fn refresh(&mut self) {
        let selected_pid = self.selected_pid();
        self.snapshot = self.sampler.sample(selected_pid);
        self.last_refresh = Instant::now();
        self.rebuild_view(selected_pid);
    }

    fn rebuild_view(&mut self, selected_pid: Option<u32>) {
        self.sort_processes();
        let filter = self.filter.trim().to_lowercase();
        self.visible = self
            .snapshot
            .processes
            .iter()
            .enumerate()
            .filter_map(|(index, process)| {
                (filter.is_empty() || process.search_text.contains(&filter)).then_some(index)
            })
            .collect();

        if self.visible.is_empty() {
            self.table_state.select(None);
            return;
        }

        let selected = selected_pid
            .and_then(|pid| {
                self.visible
                    .iter()
                    .position(|index| self.snapshot.processes[*index].pid == pid)
            })
            .or_else(|| self.table_state.selected())
            .unwrap_or(0)
            .min(self.visible.len() - 1);
        self.table_state.select(Some(selected));
    }

    fn sort_processes(&mut self) {
        let sort_key = self.sort_key;
        let sort_desc = self.sort_desc;
        self.snapshot.processes.sort_by(|left, right| {
            let ordering = match sort_key {
                SortKey::Cpu => left.cpu_usage.total_cmp(&right.cpu_usage),
                SortKey::Memory => left.memory.cmp(&right.memory),
                SortKey::Energy => left.energy_impact.total_cmp(&right.energy_impact),
                SortKey::DiskRead => left.disk_read_rate.total_cmp(&right.disk_read_rate),
                SortKey::DiskWrite => left.disk_write_rate.total_cmp(&right.disk_write_rate),
                SortKey::Name => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
                SortKey::Pid => left.pid.cmp(&right.pid),
                SortKey::User => left.user.cmp(&right.user),
                SortKey::Runtime => left.run_time.cmp(&right.run_time),
            };
            if sort_desc {
                ordering.reverse()
            } else {
                ordering
            }
        });
    }

    fn set_tab(&mut self, tab: Tab) -> bool {
        if self.tab == tab {
            return false;
        }
        self.tab = tab;
        self.sort_key = tab.default_sort();
        self.sort_desc = self.sort_key != SortKey::Name && self.sort_key != SortKey::Pid;
        self.rebuild_view(self.selected_pid());
        true
    }

    fn next_tab(&mut self) -> bool {
        let index = Tab::ALL
            .iter()
            .position(|tab| *tab == self.tab)
            .unwrap_or(0);
        self.set_tab(Tab::ALL[(index + 1) % Tab::ALL.len()])
    }

    fn previous_tab(&mut self) -> bool {
        let index = Tab::ALL
            .iter()
            .position(|tab| *tab == self.tab)
            .unwrap_or(0);
        self.set_tab(Tab::ALL[(index + Tab::ALL.len() - 1) % Tab::ALL.len()])
    }

    fn set_sort(&mut self, sort_key: SortKey, descending: bool) {
        if self.sort_key == sort_key {
            self.sort_desc = !self.sort_desc;
        } else {
            self.sort_key = sort_key;
            self.sort_desc = descending;
        }
        self.rebuild_view(self.selected_pid());
    }

    fn cycle_sort(&mut self) {
        const ORDER: [SortKey; 9] = [
            SortKey::Cpu,
            SortKey::Memory,
            SortKey::Energy,
            SortKey::DiskWrite,
            SortKey::DiskRead,
            SortKey::Name,
            SortKey::Pid,
            SortKey::User,
            SortKey::Runtime,
        ];
        let index = ORDER
            .iter()
            .position(|key| *key == self.sort_key)
            .unwrap_or(0);
        self.sort_key = ORDER[(index + 1) % ORDER.len()];
        self.sort_desc = self.sort_key != SortKey::Name
            && self.sort_key != SortKey::Pid
            && self.sort_key != SortKey::User;
        self.rebuild_view(self.selected_pid());
    }

    fn begin_kill(&mut self, intent: KillIntent) -> bool {
        if self.selected_pid().is_some() {
            self.confirm = Some(intent);
            true
        } else {
            false
        }
    }

    fn adjust_interval(&mut self, faster: bool) {
        let millis = self.interval.as_millis() as u64;
        let next = if faster {
            millis.saturating_sub(250).max(250)
        } else {
            (millis + 250).min(10_000)
        };
        self.interval = Duration::from_millis(next);
        self.notice = Some(Notice::new(format!("refresh interval: {} ms", next)));
    }

    fn select_next(&mut self, amount: usize) -> bool {
        if self.visible.is_empty() {
            return false;
        }
        let selected = self.table_state.selected().unwrap_or(0);
        let next = (selected + amount).min(self.visible.len() - 1);
        self.table_state.select(Some(next));
        next != selected
    }

    fn select_previous(&mut self, amount: usize) -> bool {
        if self.visible.is_empty() {
            return false;
        }
        let selected = self.table_state.selected().unwrap_or(0);
        let next = selected.saturating_sub(amount);
        self.table_state.select(Some(next));
        next != selected
    }

    fn select_first(&mut self) -> bool {
        if !self.visible.is_empty() {
            let changed = self.table_state.selected() != Some(0);
            self.table_state.select(Some(0));
            changed
        } else {
            false
        }
    }

    fn select_last(&mut self) -> bool {
        if !self.visible.is_empty() {
            let last = self.visible.len() - 1;
            let changed = self.table_state.selected() != Some(last);
            self.table_state.select(Some(last));
            changed
        } else {
            false
        }
    }

    fn next_poll_timeout(&self) -> Duration {
        let refresh = self.interval.saturating_sub(self.last_refresh.elapsed());
        self.notice
            .as_ref()
            .map(Notice::remaining)
            .map_or(refresh, |notice| refresh.min(notice))
    }

    fn clear_expired_notice(&mut self) -> bool {
        if self.notice.as_ref().is_some_and(Notice::expired) {
            self.notice = None;
            true
        } else {
            false
        }
    }
}
