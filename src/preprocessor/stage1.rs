use std::convert::{Infallible, TryFrom};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::preprocessor::Stage;
use crate::runtime::Worker;

pub struct Stage1 {
    code: String,
}

/// Builds from source code directly; never fails.
impl TryFrom<&str> for Stage1 {
    type Error = Infallible;

    fn try_from(code: &str) -> Result<Self, Self::Error> {
        Ok(Self {
            code: code.to_string(),
        })
    }
}

impl TryFrom<String> for Stage1 {
    type Error = Infallible;

    fn try_from(code: String) -> Result<Self, Self::Error> {
        Self::try_from(code.as_str())
    }
}

/// Reads the code from a file.
impl TryFrom<&Path> for Stage1 {
    type Error = io::Error;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        Ok(Self {
            code: fs::read_to_string(path)?,
        })
    }
}

impl TryFrom<PathBuf> for Stage1 {
    type Error = io::Error;

    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        Self::try_from(path.as_path())
    }
}

impl Stage for Stage1 {
    fn stage(&self) -> u8 {
        1
    }

    fn get_code(&self) -> String {
        self.code.clone()
    } 
}
