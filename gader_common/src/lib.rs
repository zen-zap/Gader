use core::option::Option;
use std::{
    fmt::{self, Display},
    sync::Arc,
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub service: Arc<str>,
    pub timestamp: Arc<str>,
    pub level: Arc<str>,
    pub context: Arc<str>,
    pub message: Arc<str>,
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
    },
    HandshakeAck {
        accepted: bool,
    },
    Batch(Vec<LogEntry>),
    UpdateFilter {
        service: Option<String>,
    },
}
