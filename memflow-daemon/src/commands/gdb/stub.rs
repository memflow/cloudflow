use crate::error::{Error, Result};
use crate::state::{state_lock_sync, CachedWin32Process, GdbStubHandle, KernelHandle};

use std::net::{TcpListener, TcpStream};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

use log::{error, info};
use url::Url;

use gdbstub::{
    arch, BreakOp, Connection, DisconnectReason, GdbStub, ResumeAction, StopReason, Target, Tid,
    TidSelector, SINGLE_THREAD_TID,
};

use memflow_core::*;

fn wait_for_tcp(sockaddr: &str) -> Result<TcpStream> {
    info!("started tcp gdb stub on {:?}", sockaddr);
    let sock = TcpListener::bind(sockaddr).map_err(|e| {
        error!("{}", e);
        Error::IO
    })?;

    let (stream, addr) = sock.accept().map_err(|e| {
        error!("{}", e);
        Error::IO
    })?;
    info!("debugger connected from {}", addr);
    Ok(stream)
}

#[cfg(unix)]
fn wait_for_uds(path: &str) -> Result<UnixStream> {
    match std::fs::remove_file(path) {
        Ok(_) => {}
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => {}
            _ => {
                error!("{}", e);
                return Err(Error::IO);
            }
        },
    }

    info!("started gdb stub at uds {}", path);
    let sock = UnixListener::bind(path).map_err(|e| {
        error!("{}", e);
        Error::IO
    })?;

    let (stream, addr) = sock.accept().map_err(|e| {
        error!("{}", e);
        Error::IO
    })?;
    info!("debugger connected from {:?}", addr);
    Ok(stream)
}

fn gdb_stub_init(id: &str, conn_id: &str, addr: &str) -> Result<()> {
    let mut state = state_lock_sync();
    if let Some(conn) = state.connection_mut(conn_id) {
        conn.refcount += 1;
        state
            .gdb_stubs
            .insert(id.to_string(), GdbStubHandle::new(id, conn_id, addr));
    }
    Ok(())
}

fn gdb_stub_drop(id: &str, conn_id: &str) -> Result<()> {
    let mut state = state_lock_sync();
    if state.gdb_stubs.contains_key(id) {
        info!(
            "closing gdb stub and removing reference from connection {}",
            conn_id
        );

        if let Some(conn) = state.connection_mut(conn_id) {
            conn.refcount -= 1;
        }

        state.gdb_stubs.remove(id);
    }
    Ok(())
}

fn gdb_wait_for_connection(addr: &str, mut stub: GdbStubx64) -> Result<()> {
    let url = Url::parse(addr).map_err(|_| Error::Other("invalid url"))?;
    let connection: Box<dyn Connection<Error = std::io::Error>> = match url.scheme() {
        "tcp" => {
            if let Some(host_str) = url.host_str() {
                Box::new(wait_for_tcp(&format!(
                    "{}:{}",
                    host_str,
                    url.port().unwrap_or(8000)
                ))?)
            } else {
                return Err(Error::Other("invalid tcp host"));
            }
        }
        "unix" => Box::new(wait_for_uds(url.path())?),
        _ => return Err(Error::Other("only tcp and unix urls are supported")),
    };

    // hook-up debugger
    let mut debugger = GdbStub::new(connection);
    match debugger.run(&mut stub).unwrap() {
        DisconnectReason::Disconnect => {
            info!("client disconnected");
        }
        DisconnectReason::TargetHalted => info!("target halted"),
        DisconnectReason::Kill => {
            info!("gdb sent a kill command");
        }
    };

    Ok(())
}

/// Creates a new gdb stub and blocks until the user disconnects.
/// This function will also add and remove this gdb stub from/into the global state.
pub fn spawn_gdb_stub(
    id: &str,
    conn_id: &str,
    pid: PID,
    addr: &str,
    kernel: KernelHandle,
) -> Result<()> {
    // TODO: generic stubs per architecture
    let stub = GdbStubx64::new(kernel, pid).unwrap();

    // add to global state
    gdb_stub_init(id, conn_id, addr)?;

    // we do not fail here to ensure stub_drop is called
    if let Err(e) = gdb_wait_for_connection(addr, stub) {
        error!("{}", e);
    }

    gdb_stub_drop(id, conn_id)?;
    Ok(())
}

/// Implementation of the Virtual Memory GDB Stub
pub struct GdbStubx64 {
    process: CachedWin32Process,
}

impl GdbStubx64 {
    pub fn new(kernel: KernelHandle, pid: PID) -> Result<Self> {
        match kernel {
            KernelHandle::Win32(kernel) => {
                let process = kernel.into_process_pid(pid).map_err(Error::from)?;
                Ok(Self { process })
            }
        }
    }
}

// TODO: add 32 and 64 bit stubs
impl Target for GdbStubx64 {
    type Arch = arch::x86::X86_64;
    type Error = crate::error::Error;

    fn resume(
        &mut self,
        _actions: &mut dyn Iterator<Item = (TidSelector, ResumeAction)>,
        _check_gdb_interrupt: &mut dyn FnMut() -> bool,
    ) -> Result<(Tid, StopReason<u64>)> {
        Ok((SINGLE_THREAD_TID, StopReason::Halted))
    }

    fn read_registers(&mut self, _regs: &mut arch::x86::reg::X86_64CoreRegs) -> Result<()> {
        // TODO: set eip/rip to entry point of binary (and fallback to section base)
        //
        Ok(())
    }

    fn write_registers(&mut self, _regs: &arch::x86::reg::X86_64CoreRegs) -> Result<()> {
        Ok(())
    }

    fn read_addrs(
        &mut self,
        addr: std::ops::Range<u64>,
        push_byte: &mut dyn FnMut(u8),
    ) -> Result<()> {
        let buf = self
            .process
            .virt_mem
            .virt_read_raw(addr.start.into(), (addr.end - addr.start) as usize)
            .data_part()
            .map_err(Error::from)?;

        buf.iter().for_each(|&b| {
            push_byte(b);
        });

        Ok(())
    }

    fn write_addrs(&mut self, start_addr: u64, data: &[u8]) -> Result<()> {
        self.process
            .virt_mem
            .virt_write_raw(start_addr.into(), data)
            .data_part()
            .map_err(Error::from)?;
        Ok(())
    }

    fn update_sw_breakpoint(&mut self, _addr: u64, _op: BreakOp) -> Result<bool> {
        // TODO:
        Ok(true)
    }
}
