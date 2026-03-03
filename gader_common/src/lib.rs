use core::option::Option;
use std::fmt::{self, Display};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub service: String,
    pub timestamp: String,
    pub level: String,
    pub context: String,
    pub message: String,
}

impl Display for LogEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Format: [2024-02-28 12:00] [IMMICH] [INFO] (Context) Message
        write!(
            f,
            "[{}] [{}] [{}] ({}) {}",
            self.timestamp,
            self.service.to_uppercase(),
            self.level,
            self.context,
            self.message
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum NetworkPacket {
    Handshake {
        secret_token: String,
        version: u32,
    },
    HackshakeAck {
        accepted: bool,
    },
    Batch(Vec<LogEntry>),
    UpdateFilter {
        service: Option<String>,
        level: Option<String>,
    },
    KeepAlive,
}
