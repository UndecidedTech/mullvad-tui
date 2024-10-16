use std::fmt::Display;

use crossterm::event::{self, Event, KeyEvent, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style, Stylize},
    symbols::border,
    text::{Line, Span},
    widgets::{
        block::{Position, Title},
        Block, List, ListItem, ListState,
    },
    DefaultTerminal, Frame,
};

use crate::{config, error::AppError};

#[derive(Debug, Default)]
enum View {
    #[default]
    Countries,
    Cities,
    Connection,
}

#[derive(Debug, Default)]
enum InputMode {
    #[default]
    Normal,
    Search,
}

impl Display for InputMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputMode::Normal => write!(f, "Normal"),
            InputMode::Search => write!(f, "Search"),
        }
    }
}

#[derive(Debug, Default)]
pub struct App {
    countries: Vec<String>,
    cities: Vec<String>,

    connection_output: Vec<String>,
    connected: bool,

    search_string: String,

    view_mode: View,
    input_mode: InputMode,
    state: ListState,

    awaiting_second_g: bool,

    country_index: usize,
    city_index: usize,

    config: config::Config,

    exit: bool,
}

impl App {
    pub fn init(config: Option<String>) -> Result<Self, AppError> {
        println!("Initializing app...");

        let config = match config::Config::load(config.as_deref()) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("Failed to load config: {:?}", e);
                return Err(AppError::Config(e.to_string()));
            }
        };

        println!("Config loaded successfully.");

        // Check if Mullvad CLI is available
        let status = match std::process::Command::new("mullvad")
            .arg("relay")
            .arg("list")
            .status() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to execute 'mullvad relay list': {:?}", e);
                return Err(AppError::Command(e.to_string()));
            }
        };

        if status.success() {
            println!("The 'mullvad relay list' command ran successfully.");
        } else {
            eprintln!("Error: The 'mullvad relay list' command failed to run.");
            eprintln!("Please make sure Mullvad CLI is installed and accessible.");
            return Err(AppError::Command(status.to_string()));
        }

        let output = match std::process::Command::new("mullvad")
            .arg("relay")
            .arg("list")
            .output() {
            Ok(o) => o,
            Err(e) => {
                eprintln!("Failed to get output from 'mullvad relay list': {:?}", e);
                return Err(AppError::Command(e.to_string()));
            }
        };

        let countries: Vec<String> = match String::from_utf8(output.stdout) {
            Ok(s) => s.lines()
                .filter_map(|line| {
                    if line.contains('(') && !line.starts_with('\t') {
                        Some(line.trim().to_string())
                    } else {
                        None
                    }
                })
                .collect(),
            Err(e) => {
                eprintln!("Failed to parse 'mullvad relay list' output: {:?}", e);
                return Err(AppError::Parse(e.to_string()));
            }
        };

        println!("Countries list loaded successfully.");

        let output = match std::process::Command::new("mullvad")
            .arg("status")
            .output() {
            Ok(o) => o,
            Err(e) => {
                eprintln!("Failed to get Mullvad status: {:?}", e);
                return Err(AppError::Command(e.to_string()));
            }
        };

        let connection_status = String::from_utf8(output.stdout)
            .map(|s| s.contains("Connected"))
            .unwrap_or(false);

        println!("Connection status: {}", if connection_status { "Connected" } else { "Disconnected" });

        let mut state = ListState::default();
        state.select(Some(0));

        Ok(Self {
            countries,
            cities: vec![],
            connection_output: vec![],
            connected: connection_status,
            search_string: String::default(),
            country_index: 0,
            city_index: 0,
            input_mode: InputMode::default(),
            view_mode: View::default(),
            awaiting_second_g: false,
            state,
            config,
            exit: false,
        })
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<(), AppError> {
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }


    fn set_countries(&mut self) -> Result<(), AppError> {
        let output = std::process::Command::new("mullvad")
            .arg("relay")
            .arg("list")
            .output()?;

        self.countries = String::from_utf8(output.stdout)?
            .lines()
            .filter_map(|line| {
                if line.contains('(') && !line.starts_with('\t') {
                    Some(line.trim().to_string())
                } else {
                    None
                }
            })
        .collect();

                Ok(())
    }

    fn set_cities(&mut self) -> Result<(), AppError> {
        let output = std::process::Command::new("mullvad")
            .arg("relay")
            .arg("list")
            .output()?;

        let country = &self.countries[self.country_index];
        let _country_code = country.split('(').nth(1).unwrap().split(')').next().unwrap();

        self.cities = String::from_utf8(output.stdout)?
            .lines()
            .skip_while(|line| !line.contains(country))
            .skip(1)
            .take_while(|line| line.starts_with('\t'))
            .filter_map(|line| {
                if line.contains('(') && line.contains(')') {
                    Some(line.trim().to_string())
                } else {
                    None
                }
            })
        .collect();

        Ok(())
    }

    // fn set_countries(&mut self) -> Result<(), AppError> {
    //     let output = std::process::Command::new("mullvad")
    //         .arg("relay list")
    //         .output()?;
    //
    //     self.countries = String::from_utf8(output.stdout)?
    //         .split_whitespace()
    //         .map(|s| s.to_string())
    //         .collect();
    //
    //     Ok(())
    // }
    //
    // fn set_cities(&mut self) -> Result<(), AppError> {
    //     let output = std::process::Command::new("mullvad")
    //         .arg("cities")
    //         .arg(&self.countries[self.country_index])
    //         .output()?;
    //
    //     self.cities = String::from_utf8(output.stdout)?
    //         .split_whitespace()
    //         .map(|s| s.to_string())
    //         .collect();
    //
    //     Ok(())
    // }

    fn connect(&mut self) -> Result<View, AppError> {
        // mullvad relay set location se mma
        let _relay = std::process::Command::new("mullvad")
            .arg("relay set location")
            .arg(&self.countries[self.country_index])
            .arg(&self.cities[self.city_index])
            .output()?;

        let output = std::process::Command::new("mullvad")
            .arg("connect")
            .output()?;

        self.connected = output.status.success();
        self.connection_output = String::from_utf8(output.stdout)?
            .lines()
            .map(|s| s.to_string())
            .collect();

        if output.status.success() {
            Ok(View::Connection)
        } else {
            Err(AppError::Command(output.status.to_string()))
        }
    }

    fn disconnect(&mut self) -> Result<(), AppError> {
        let output = std::process::Command::new("mullvad")
            .arg("disconnect")
            .output()?;

        self.connected = !output.status.success();
        self.connection_output = String::from_utf8(output.stdout)?
            .lines()
            .map(|s| s.to_string())
            .collect();

        if output.status.success() {
            Ok(())
        } else {
            Err(AppError::Command(output.status.to_string()))
        }
    }

    fn draw(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints(
                [
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
                ]
                .as_ref(),
            )
            .split(f.area());

        let title_text = if self.connected {
            Line::from("Connected").style(Style::default().fg(self.config.colors.connected))
        } else {
            Line::from("Disconnected").style(Style::default().fg(self.config.colors.disconnected))
        };

        let title = Title::from(title_text.alignment(Alignment::Center));

        let instructions = match self.input_mode {
            InputMode::Normal => Title::from(
                Line::from(vec![
                    " Normal | ".bold(),
                    " Select ".bold(),
                    "<Enter>".into(),
                    " Down ".bold(),
                    "<J | Down>".into(),
                    " Up ".bold(),
                    "<K | Up>".into(),
                    " Quit ".bold(),
                    "<Q | Esc>".into(),
                    " Disconnect ".bold(),
                    "<D>".into(),
                ])
                .style(Style::default().fg(self.config.colors.normal_mode)),
            ),
            InputMode::Search => {
                let search_text = format!(" Search: {} | ", self.search_string);
                let instructions = vec![
                    search_text.into(),
                    " Type ".bold(),
                    "<Esc>".into(),
                    " to exit search mode ".bold(),
                    " Type ".bold(),
                    "<Backspace>".into(),
                    " to delete ".bold(),
                ];
                Title::from(
                    Line::from(instructions)
                    .style(Style::default().fg(self.config.colors.search_mode)),
                )
            }
        };

        let block = Block::bordered()
            .title(title.alignment(Alignment::Center))
            .title(
                instructions
                .alignment(Alignment::Center)
                .position(Position::Bottom),
            )
            .bg(self.config.colors.background)
            .border_set(border::THICK);

        match self.view_mode {
            View::Countries | View::Cities => self.draw_lists(f, chunks[1], block),
            View::Connection => self.draw_connection(f, chunks[1], block),
        }
    }

    fn draw_lists(&mut self, f: &mut Frame, _area: Rect, block: Block) {
        let mut list = Vec::<ListItem>::new();

        let l = match self.view_mode {
            View::Countries => self
                .countries
                .clone()
                .into_iter()
                .filter(|c| c.to_lowercase().contains(&self.search_string))
                .collect(),
            View::Cities => self
                .cities
                .clone()
                .into_iter()
                .filter(|c| c.to_lowercase().contains(&self.search_string))
                .collect(),
            _ => Vec::new(),
        };
        for (i, country) in l.iter().enumerate() {
            let style = match self.view_mode {
                View::Countries => {
                    if i == self.country_index {
                        Style::default().fg(self.config.colors.items_selected)
                    } else {
                        Style::default().fg(self.config.colors.items)
                    }
                }
                View::Cities => {
                    if i == self.city_index {
                        Style::default().fg(self.config.colors.items_selected)
                    } else {
                        Style::default().fg(self.config.colors.items)
                    }
                }
                _ => Style::default().fg(self.config.colors.items),
            };
            list.push(ListItem::new(
                    Line::from(Span::from(country.to_string()))
                    .alignment(Alignment::Center)
                    .style(style),
            ));
        }

        let list = List::new(list).block(block).highlight_style(
            Style::default()
            .add_modifier(Modifier::BOLD)
            .add_modifier(Modifier::ITALIC),
        );
        f.render_stateful_widget(list, f.area(), &mut self.state);
    }

    fn draw_connection(&mut self, f: &mut Frame, _area: Rect, block: Block) {
        let mut list = Vec::<ListItem>::new();

        for line in self.connection_output.iter() {
            list.push(ListItem::new(
                    Line::from(Span::from(line.to_string()))
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(self.config.colors.connection_output)),
            ));
        }

        let list = List::new(list).block(block).highlight_style(
            Style::default()
            .add_modifier(Modifier::BOLD)
            .add_modifier(Modifier::ITALIC),
        );
        f.render_widget(list, f.area());
    }

    fn handle_events(&mut self) -> Result<(), AppError> {
        if let Event::Key(key_event) = event::read()? {
            if key_event.kind == KeyEventKind::Press {
                self.handle_key_event(key_event)?
            }
        }
        Ok(())
    }

    fn handle_key_event(&mut self, event: KeyEvent) -> Result<(), AppError> {
        match self.input_mode {
            InputMode::Normal => self.handle_normal_mode(event)?,
            InputMode::Search => self.handle_search_mode(event)?,
        }
        Ok(())
    }

    fn handle_normal_mode(&mut self, event: KeyEvent) -> Result<(), AppError> {
        match event.code {
            event::KeyCode::Esc | event::KeyCode::Char('q') => self.exit = true,
            event::KeyCode::Enter => {
                self.search_string.clear();
                self.view_mode = match self.view_mode {
                    View::Countries => {
                        self.state.select(Some(0));
                        self.set_cities()?;
                        self.city_index = 0;
                        View::Cities
                    }
                    View::Cities => self.connect()?,
                    View::Connection => {
                        self.country_index = 0;
                        self.city_index = 0;
                        self.set_countries()?;
                        View::Countries
                    }
                };
            }
            event::KeyCode::Char('D') => self.disconnect()?,
            event::KeyCode::Down | event::KeyCode::Char('j') => self.increment_index(),
            event::KeyCode::Up | event::KeyCode::Char('k') => self.decrement_index(),
            event::KeyCode::Char('G') => match self.view_mode {
                View::Countries => {
                    self.country_index = self.countries.len() - 1;
                    self.state.select(Some(self.country_index));
                }
                View::Cities => {
                    self.city_index = self.cities.len() - 1;
                    self.state.select(Some(self.city_index))
                }
                _ => {}
            },
            //TODO: fix `gg` keybind
            event::KeyCode::Char('g') => {
                if self.awaiting_second_g {
                    match self.view_mode {
                        View::Countries => {
                            self.country_index = 0;
                            self.state.select(Some(self.country_index));
                        }
                        View::Cities => {
                            self.city_index = 0;
                            self.state.select(Some(self.city_index));
                        }
                        _ => {}
                    }
                    self.awaiting_second_g = false;
                } else {
                    self.awaiting_second_g = true;
                }
            }
            event::KeyCode::Char('/') | event::KeyCode::Char('i') => {
                self.input_mode = InputMode::Search;
            }
            event::KeyCode::Char('h') => match self.view_mode {
                View::Cities => {
                    self.set_countries()?;
                    self.city_index = 0;
                    self.view_mode = View::Countries;
                }
                View::Connection => {
                    self.set_cities()?;
                    self.city_index = 0;
                    self.view_mode = View::Cities;
                }
                _ => {}
            },
            _ => {}
        }
        Ok(())
    }

    fn handle_search_mode(&mut self, event: KeyEvent) -> Result<(), AppError> {
        match event.code {
            event::KeyCode::Enter => {
                self.input_mode = InputMode::Normal;
                self.view_mode = match self.view_mode {
                    View::Countries => {
                        self.set_cities()?;
                        self.search_string.clear();
                        View::Cities
                    }
                    View::Cities => View::Cities, // self.connect()?,
                    View::Connection => {
                        self.country_index = 0;
                        View::Countries
                    }
                };
            }
            event::KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
            }
            event::KeyCode::Char(c) => {
                self.search_string.push(c);
                self.state.select(Some(0));
                match self.view_mode {
                    View::Countries => {
                        self.countries = self
                            .countries
                            .clone()
                            .into_iter()
                            .filter(|c| c.to_lowercase().contains(&self.search_string))
                            .collect();
                        }
                    View::Cities => {
                        self.cities = self
                            .cities
                            .clone()
                            .into_iter()
                            .filter(|c| c.to_lowercase().contains(&self.search_string))
                            .collect();
                        }
                    View::Connection => {}
                }
                self.country_index = 0;
            }
            event::KeyCode::Backspace => {
                self.search_string.pop();
                self.set_countries()?;
                self.country_index = 0;
            }
            _ => {}
        }
        Ok(())
    }

    fn decrement_country(&mut self) {
        if self.country_index > 0 {
            self.country_index -= 1;
        }
        self.state.select(Some(self.country_index));
    }

    fn increment_country(&mut self) {
        if self.country_index < self.countries.len() - 1 {
            self.country_index += 1;
        }
        self.state.select(Some(self.country_index));
    }

    fn decrement_city(&mut self) {
        if self.city_index > 0 {
            self.city_index -= 1;
        }
        self.state.select(Some(self.city_index));
    }

    fn increment_city(&mut self) {
        if self.city_index < self.cities.len() - 1 {
            self.city_index += 1;
        }
        self.state.select(Some(self.city_index));
    }

    fn decrement_index(&mut self) {
        match self.view_mode {
            View::Countries => self.decrement_country(),
            View::Cities => self.decrement_city(),
            _ => {}
        }
    }

    fn increment_index(&mut self) {
        match self.view_mode {
            View::Countries => self.increment_country(),
            View::Cities => self.increment_city(),
            _ => {}
        }
    }
}
