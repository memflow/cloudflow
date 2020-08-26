use crate::error::{Error, Result};
use crate::state::{state_lock_sync, KernelHandle};

use std::net::{TcpListener, TcpStream};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

use gdbstub::{
    arch, BreakOp, Connection, DisconnectReason, GdbStub, OptResult, ResumeAction, StopReason,
    Target, Tid, TidSelector, WatchKind, SINGLE_THREAD_TID,
};

use memflow_core::*;
use memflow_win32::*;

// TODO: better error handling
fn wait_for_tcp(port: u16) -> Result<TcpStream> {
    //let sockaddr = format!("0.0.0.0:{}", port);
    let sockaddr = format!("127.0.0.1:{}", port);
    eprintln!("Waiting for a GDB connection on {:?}...", sockaddr);

    let sock = TcpListener::bind(sockaddr).map_err(|_| Error::IO)?;
    let (stream, addr) = sock.accept().map_err(|_| Error::IO)?;
    eprintln!("Debugger connected from {}", addr);

    Ok(stream)
}

// TODO: better error handling
#[cfg(unix)]
fn wait_for_uds(path: &str) -> Result<UnixStream> {
    match std::fs::remove_file(path) {
        Ok(_) => {}
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => {}
            _ => return Err(Error::IO),
        },
    }

    eprintln!("Waiting for a GDB connection on {}...", path);

    let sock = UnixListener::bind(path).map_err(|_| Error::IO)?;
    let (stream, addr) = sock.accept().map_err(|_| Error::IO)?;
    eprintln!("Debugger connected from {:?}", addr);

    Ok(stream)
}

/// Creates a new gdb stub and blocks
pub fn spawn_stub(id: &str, conn_id: &str, pid: PID, addr: &str) -> Result<()> {
    // TODO: parse addr
    let args = ConnectorArgs::try_parse_str(addr)?;

    let connection: Box<dyn Connection<Error = std::io::Error>> = {
        //Box::new(wait_for_uds("/tmp/memflow_gdb")?)
        Box::new(wait_for_tcp(8000)?)

        /*
        if std::env::args().nth(1) == Some("--uds".to_string()) {
            #[cfg(not(unix))]
            {
                return Err("Unix Domain Sockets can only be used on Unix".into());
            }
            #[cfg(unix)]
            {
                Box::new(wait_for_uds("/tmp/memflow_gdb")?)
            }
        } else {
            Box::new(wait_for_tcp(9001)?)
        }
        */
    };

    // hook-up debugger
    let mut debugger = GdbStub::new(connection);

    let mut stub = VMGDBStub::new(id, conn_id, pid);

    match debugger.run(&mut stub).map_err(|_| Error::GDB)? {
        DisconnectReason::Disconnect => {
            // run to completion
            //while emu.step() != Some(emu::Event::Halted) {}
            println!("disconnected");
            return Ok(());
        }
        DisconnectReason::TargetHalted => println!("Target halted!"),
        DisconnectReason::Kill => {
            println!("GDB sent a kill command!");
            return Ok(());
        }
    }

    Ok(())
}

///
pub struct VMGDBStub {
    id: String,
    conn_id: String,
    pid: PID, // ?
}

impl VMGDBStub {
    pub fn new(id: &str, conn_id: &str, pid: PID) -> Self {
        Self {
            id: id.to_string(),
            conn_id: conn_id.to_string(),
            pid,
        }
    }
}

// TODO: add 32 and 64 bit stubs
impl Target for VMGDBStub {
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
        // TODO: set eip/rip to entry point of binary
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
        println!("read_addrs: {:?}", addr);
        /*
        for addr in addr {
            push_byte(self.mem.r8(addr))
        }
        */

        let mut state = state_lock_sync();
        let conn = state
            .connection_mut(&self.conn_id)
            .ok_or_else(|| Error::Other("connection not found"))?;

        match &mut conn.kernel {
            KernelHandle::Win32(kernel) => {
                let mut process = kernel.process_pid(self.pid).map_err(Error::from)?;
                let buf = process
                    .virt_mem
                    .virt_read_raw(addr.start.into(), (addr.end - addr.start) as usize)
                    .data_part()
                    .map_err(Error::from)?;

                buf.iter().for_each(|&b| {
                    push_byte(b);
                });
            }
        }

        Ok(())
    }

    fn write_addrs(&mut self, start_addr: u64, data: &[u8]) -> Result<()> {
        println!(
            "write_addrs: {}..{}",
            start_addr,
            start_addr + data.len() as u64
        );
        /*
        for (addr, val) in (start_addr..).zip(data.iter().copied()) {
            self.mem.w8(addr, val)
        }
        */
        Ok(())
    }

    fn update_sw_breakpoint(&mut self, _addr: u64, _op: BreakOp) -> Result<bool> {
        /*
        match op {
            BreakOp::Add => self.breakpoints.push(addr),
            BreakOp::Remove => {
                let pos = match self.breakpoints.iter().position(|x| *x == addr) {
                    None => return Ok(false),
                    Some(pos) => pos,
                };
                self.breakpoints.remove(pos);
            }
        }
        */

        Ok(true)
    }
}
