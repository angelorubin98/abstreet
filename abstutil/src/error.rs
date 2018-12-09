use std::{error, fmt};

pub struct Error {
    message: String,
    context: Vec<String>,
}

impl Error {
    pub fn new(message: String) -> Error {
        Error {
            message,
            context: Vec::new(),
        }
    }

    pub fn context(mut self, msg: String) -> Error {
        self.context.push(msg);
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "\n{}", self.message)?;
        for c in &self.context {
            writeln!(f, "  - {}", c)?;
        }
        writeln!(f)?;
        Ok(())
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Do the same thing as the Display trait
        write!(f, "{}", self)
    }
}

impl error::Error for Error {}
