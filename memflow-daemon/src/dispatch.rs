use crate::dto::response;
use crate::error::{Error, Result};

use log::Level;

use futures::prelude::*;
use futures::Sink;
use std::marker::Unpin;

pub async fn send_log_debug<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: &str,
) -> Result<()> {
    frame
        .send(response::Message::Log(response::Log {
            level: Level::Debug as i32,
            msg: msg.to_string(),
        }))
        .await
        .map_err(|_| Error::IO)
}

pub async fn send_log_info<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: &str,
) -> Result<()> {
    frame
        .send(response::Message::Log(response::Log {
            level: Level::Info as i32,
            msg: msg.to_string(),
        }))
        .await
        .map_err(|_| Error::IO)
}

pub async fn send_log_warn<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: &str,
) -> Result<()> {
    frame
        .send(response::Message::Log(response::Log {
            level: Level::Warn as i32,
            msg: msg.to_string(),
        }))
        .await
        .map_err(|_| Error::IO)
}

pub async fn send_log_error<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: &str,
) -> Result<()> {
    frame
        .send(response::Message::Log(response::Log {
            level: Level::Error as i32,
            msg: msg.to_string(),
        }))
        .await
        .map_err(|_| Error::IO)
}

pub async fn send_table<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    table: response::Table,
) -> Result<()> {
    frame
        .send(response::Message::Table(table))
        .await
        .map_err(|_| Error::IO)
}

pub async fn send_eof<S: Sink<response::Message> + Unpin>(frame: &mut S) -> Result<()> {
    frame
        .send(response::Message::EOF)
        .await
        .map_err(|_| Error::IO)
}
