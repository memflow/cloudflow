#![allow(unused)]

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

pub async fn send_phys_mem_read<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    reads: Vec<response::PhysicalMemoryReadEntry>,
) -> Result<()> {
    frame
        .send(response::Message::PhysicalMemoryRead(
            response::PhysicalMemoryRead { reads },
        ))
        .await
        .map_err(|_| Error::IO)
}

pub async fn send_phys_mem_metadata<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    metadata: memflow::PhysicalMemoryMetadata,
) -> Result<()> {
    frame
        .send(response::Message::PhysicalMemoryMetadata(
            response::PhysicalMemoryMetadata { metadata },
        ))
        .await
        .map_err(|_| Error::IO)
}

pub async fn send_ok<S: Sink<response::Message> + Unpin>(frame: &mut S) -> Result<()> {
    frame
        .send(response::Message::Result(response::CommandResult {
            success: true,
            msg: "".to_string(),
        }))
        .await
        .map_err(|_| Error::IO)
}

pub async fn send_err<S: Sink<response::Message> + Unpin>(frame: &mut S, msg: &str) -> Result<()> {
    frame
        .send(response::Message::Result(response::CommandResult {
            success: false,
            msg: msg.to_string(),
        }))
        .await
        .map_err(|_| Error::IO)
}
