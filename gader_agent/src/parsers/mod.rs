use gader_common::LogEntry;

pub mod immich;
pub mod vaultwarden;

pub trait LogParser {
    /// Parses the logs specific to a service and returns them as `LogEntry`
    fn parse(&self, line: &str) -> Option<LogEntry>;
}
