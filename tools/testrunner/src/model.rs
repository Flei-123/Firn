#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pass,
    Fail,
    Skip,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Fail => "fail",
            Status::Skip => "skip",
        }
    }
}

pub struct Case {
    pub name: String,
    pub file: String,
    pub level: String,
    pub kind: &'static str,
    pub status: Status,
    pub message: String,
    pub micros: u128,
    pub line: u32,
    pub col: u32,
}

impl Case {
    pub fn new(file: &str, level: &str, kind: &'static str) -> Case {
        Case {
            name: file.to_string(),
            file: file.to_string(),
            level: level.to_string(),
            kind,
            status: Status::Pass,
            message: String::new(),
            micros: 0,
            line: 1,
            col: 1,
        }
    }

    pub fn named(mut self, name: String) -> Case {
        self.name = name;
        self
    }

    pub fn at(mut self, line: u32, col: u32) -> Case {
        self.line = line;
        self.col = col;
        self
    }

    pub fn passed(mut self, micros: u128) -> Case {
        self.status = Status::Pass;
        self.micros = micros;
        self
    }

    pub fn failed(mut self, message: String, micros: u128) -> Case {
        self.status = Status::Fail;
        self.message = message;
        self.micros = micros;
        self
    }

    pub fn skipped(mut self, reason: &str) -> Case {
        self.status = Status::Skip;
        self.message = reason.to_string();
        self
    }

    pub fn millis(&self) -> u128 {
        self.micros / 1000
    }
}

pub struct Summary {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
}

impl Summary {
    pub fn of(cases: &[Case]) -> Summary {
        let count = |s: Status| cases.iter().filter(|c| c.status == s).count();
        Summary {
            passed: count(Status::Pass),
            failed: count(Status::Fail),
            skipped: count(Status::Skip),
        }
    }

    pub fn total(&self) -> usize {
        self.passed + self.failed
    }

    pub fn rate(&self) -> f64 {
        if self.total() == 0 {
            0.0
        } else {
            self.passed as f64 / self.total() as f64
        }
    }
}
