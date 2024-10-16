#![allow(dead_code)]

#[derive(Debug)]
pub enum AppError {
    Io(std::io::Error),
    Command(String),
    Utf8(std::string::FromUtf8Error),
    Toml(toml::de::Error),
    ParseColor(ratatui::style::ParseColorError),
    Config(String),
    Parse(String),
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<std::string::FromUtf8Error> for AppError {
    fn from(e: std::string::FromUtf8Error) -> Self {
        Self::Utf8(e)
    }
}

impl From<toml::de::Error> for AppError {
    fn from(e: toml::de::Error) -> Self {
        Self::Toml(e)
    }
}

impl From<ratatui::style::ParseColorError> for AppError {
    fn from(e: ratatui::style::ParseColorError) -> Self {
        Self::ParseColor(e)
    }
}

impl From<std::process::ExitStatus> for AppError {
    fn from(e: std::process::ExitStatus) -> Self {
        Self::Command(e.to_string())
    }
}


