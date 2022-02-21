use cglue::arc::CArcSome;
use cglue::callback::OpaqueCallback;
use cglue::result::IntError;
use cglue::slice::{CSliceMut, CSliceRef};
use cglue::tuple::CTup2;
use core::mem::size_of;
use core::num::NonZeroI32;
use filer::prelude::v1::*;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, BufReader};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{tcp, TcpListener, TcpStream};
use tokio::runtime::{Handle, Runtime};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

#[derive(Default)]
struct FragmentBuffer<'a> {
    bufs: Vec<Box<[u8]>>,
    fragments: BTreeMap<usize, Vec<*mut u8>>,
    _phantom: core::marker::PhantomData<&'a mut [u8]>,
}

unsafe impl Send for FragmentBuffer<'_> {}

impl<'a> FragmentBuffer<'a> {
    pub unsafe fn put_back(&mut self, fragment: *mut [u8]) {
        let len = (*fragment).len();
        let raw = (*fragment).as_mut_ptr();
        self.fragments.entry(len).or_default().push(raw);
    }

    pub fn get(&mut self, size: usize) -> &'a mut [u8] {
        let fragment = if let Some((&sz, fragments)) = self.fragments.range_mut(size..).next() {
            let frag = fragments.pop().unwrap();
            if fragments.is_empty() {
                self.fragments.remove(&sz);
            }
            if sz > size {
                unsafe {
                    self.put_back(core::slice::from_raw_parts_mut(frag.add(size), sz - size));
                }
            }
            frag
        } else {
            let mut b = vec![0; size].into_boxed_slice();
            let raw = b.as_mut_ptr();
            self.bufs.push(b);
            raw
        };
        unsafe { core::slice::from_raw_parts_mut(fragment, size) }
    }
}

#[derive(Default)]
struct SegmentTree<'a> {
    segments: BTreeMap<Size, Vec<(*mut u8, usize)>>,
    _phantom: core::marker::PhantomData<&'a mut [u8]>,
}

unsafe impl Send for SegmentTree<'_> {}

impl<'a> SegmentTree<'a> {
    pub fn get(&mut self, start: Size, len: usize) -> Option<&'a mut [u8]> {
        let mut iter = self.segments.range_mut(..=start);

        while let Some((&seg_start, segs)) = iter.next_back() {
            for i in 0..segs.len() {
                let seg_end = seg_start + segs[i].1 as Size;
                let end = start + len as Size;

                if seg_end > start {
                    let (seg, seg_sz) = segs.swap_remove(i);

                    // Add anything on the left
                    if seg_start < start {
                        self.add_seg(seg_start, unsafe {
                            core::slice::from_raw_parts_mut(seg, (start - seg_start) as usize)
                        });
                    } else {
                        // Otherwise cleanup, if this was the last segment we took
                        if segs.is_empty() {
                            self.segments.remove(&seg_start);
                        }
                    }

                    let seg = unsafe { seg.add((start - seg_start) as usize) };

                    // Add anything over the end
                    if seg_end > end {
                        self.add_seg(end, unsafe {
                            core::slice::from_raw_parts_mut(seg.add(len), (seg_end - end) as usize)
                        });
                    }

                    return Some(unsafe {
                        core::slice::from_raw_parts_mut(
                            seg,
                            (core::cmp::min(seg_end, end) - start) as usize,
                        )
                    });
                }
            }
        }

        None
    }

    pub fn add_seg(&mut self, start: Size, seg: &'a mut [u8]) {
        self.segments
            .entry(start)
            .or_default()
            .push((seg.as_mut_ptr(), seg.len()))
    }
}

#[repr(u8)]
pub enum FrontendFuncs {
    Read = 0,
    Write,
    Rpc,
    Close,
    Open,
    Metadata,
    List,
    Highest, // fn read(&self, handle: usize, data: VecOps<RWData>) -> Result<()>;
             // /// Perform write operation on the given handle.
             // fn write(&self, handle: usize, data: VecOps<ROData>) -> Result<()>;
             // /// Perform remote procedure call on the given handle.
             // fn rpc(&self, handle: usize, input: &[u8], output: &mut [u8]) -> Result<()>;
             // /// Close an already open handle.
             // fn close(&self, handle: usize) -> Result<()>;
             // /// Open a leaf at the given path. The result is a handle.
             // fn open(&self, path: &str) -> Result<usize>;
             // /// Get metadata of given path.
             // fn metadata(&self, path: &str) -> Result<NodeMetadata>;
             // /// List entries in the given path. It is a (name, is_branch) pair.
             // fn list(&self, path: &str, out: &mut OpaqueCallback<ListEntry>) -> Result<()>;
}

pub trait SplitStream {
    type OwnedReadHalf: AsyncRead + Send + Unpin;
    type OwnedWriteHalf: AsyncWrite + Send + Unpin;

    fn into_split(self) -> (Self::OwnedReadHalf, Self::OwnedWriteHalf);
}

impl SplitStream for TcpStream {
    type OwnedReadHalf = tcp::OwnedReadHalf;
    type OwnedWriteHalf = tcp::OwnedWriteHalf;

    fn into_split(self) -> (Self::OwnedReadHalf, Self::OwnedWriteHalf) {
        TcpStream::into_split(self)
    }
}

#[async_trait::async_trait]
pub trait Listener {
    type Stream: SplitStream + Unpin + Send + 'static;
    type SocketAddr;

    async fn accept(&self) -> io::Result<(Self::Stream, Self::SocketAddr)>;
}

#[async_trait::async_trait]
impl Listener for TcpListener {
    type Stream = TcpStream;
    type SocketAddr = std::net::SocketAddr;

    async fn accept(&self) -> io::Result<(Self::Stream, Self::SocketAddr)> {
        TcpListener::accept(self).await
    }
}

pub struct FilerClient<T> {
    stream: Mutex<T>,
    runtime: Runtime,
}

impl<T: AsyncRead + AsyncWrite + Unpin> Frontend for FilerClient<T> {
    /// Perform read operation on the given handle
    fn read(&self, handle: usize, mut data: VecOps<RWData>) -> Result<()> {
        self.runtime.block_on(async {
            let mut stream = self.stream.lock().await;
            let mut bufs = SegmentTree::default();

            stream.write_all(&[FrontendFuncs::Read as u8]).await?;

            for CTup2(addr, buf) in data.inp {
                stream.write_all(&(addr as Size).to_le_bytes()).await?;
                stream.write_all(&(buf.len() as Size).to_le_bytes()).await?;
                stream.write_all(&buf).await?;
                bufs.add_seg(addr, buf.into());
            }

            stream.write_all(&(0 as Size).to_le_bytes()).await?;
            stream.write_all(&(0 as Size).to_le_bytes()).await?;

            loop {
                let mut idx = [0u8];
                stream.read_exact(&mut idx).await?;
                match idx[0] {
                    0 => {
                        let mut err = [0u8; size_of::<i32>()];
                        stream.read_exact(&mut err).await?;
                        return NonZeroI32::new(i32::from_le_bytes(err))
                            .map(|v| Err(Error::from_int_err(v)))
                            .unwrap_or(Ok(()));
                    }
                    1 => {
                        let mut addr = [0u8; size_of::<Size>()];
                        let mut buf_len = [0u8; size_of::<Size>()];
                        stream.read_exact(&mut addr).await?;
                        stream.read_exact(&mut buf_len).await?;
                        let mut addr = Size::from_le_bytes(addr);
                        let mut buf_len = Size::from_le_bytes(buf_len) as usize;
                        while buf_len > 0 {
                            let buf = bufs.get(addr, buf_len).unwrap();
                            let blen = buf.len();
                            stream.read_exact(buf).await?;
                            opt_call(data.out.as_deref_mut(), CTup2(addr, buf.into()));
                            addr += blen as Size;
                            buf_len -= blen;
                        }
                    }
                    2 => {
                        let mut addr = [0u8; size_of::<Size>()];
                        let mut buf_len = [0u8; size_of::<Size>()];
                        let mut err = [0u8; size_of::<i32>()];
                        stream.read_exact(&mut addr).await?;
                        stream.read_exact(&mut buf_len).await?;
                        stream.read_exact(&mut err).await?;
                        let mut addr = Size::from_le_bytes(addr);
                        let mut buf_len = Size::from_le_bytes(buf_len) as usize;
                        let mut err = i32::from_le_bytes(err) as usize;
                        while buf_len > 0 {
                            let buf = bufs.get(addr, buf_len).unwrap();
                            let blen = buf.len();
                            opt_call(data.out.as_deref_mut(), CTup2(addr, buf.into()));
                            addr += blen as Size;
                            buf_len -= blen;
                        }
                    }
                    _ => {}
                }
            }

            Ok(())
        })
    }
    /// Perform write operation on the given handle.
    fn write(&self, handle: usize, data: VecOps<ROData>) -> Result<()> {
        todo!()
    }
    /// Perform remote procedure call on the given handle.
    fn rpc(&self, handle: usize, input: &[u8], output: &mut [u8]) -> Result<()> {
        todo!()
    }
    /// Close an already open handle.
    fn close(&self, handle: usize) -> Result<()> {
        todo!()
    }
    /// Open a leaf at the given path. The result is a handle.
    fn open(&self, path: &str) -> Result<usize> {
        todo!()
    }
    /// Get metadata of given path.
    fn metadata(&self, path: &str) -> Result<NodeMetadata> {
        todo!()
    }
    /// List entries in the given path. It is a (name, is_branch) pair.
    fn list(&self, path: &str, out: &mut OpaqueCallback<ListEntry>) -> Result<()> {
        todo!()
    }
}

pub struct FilerServer<T: Listener> {
    listener: T,
    clients: Vec<(JoinHandle<io::Result<()>>, T::SocketAddr)>,
    node: CArcSome<Node>,
}

impl<T: Listener> FilerServer<T> {
    pub async fn run(mut self) -> io::Result<()> {
        loop {
            let (mut socket, addr) = self.listener.accept().await?;

            let node = self.node.clone();

            let handle = tokio::spawn(async move {
                let (reader, mut writer) = socket.into_split();
                let mut reader = BufReader::new(reader);

                // In a loop, read data from the socket and write the data back.
                loop {
                    let cmd = &mut [0u8];
                    reader.read_exact(cmd).await?;
                    let cmd = cmd[0];

                    if cmd < FrontendFuncs::Highest as u8 {
                        use FrontendFuncs::*;
                        match unsafe { core::mem::transmute::<_, FrontendFuncs>(cmd) } {
                            Read => {
                                let mut bufs = FragmentBuffer::default();
                                let bufs = Mutex::new(bufs);
                                let writer = Mutex::new(&mut writer);

                                let mut fh = [0u8; size_of::<Size>()];
                                reader.read_exact(&mut fh).await?;
                                let fh = Size::from_le_bytes(fh) as usize;

                                let handle = Handle::current();

                                let iter = &mut core::iter::from_fn(|| {
                                    let mut addr = [0u8; size_of::<Size>()];

                                    let buf = handle
                                        .block_on(async {
                                            let mut size = [0u8; size_of::<Size>()];
                                            reader.read_exact(&mut addr).await?;
                                            let sz = reader
                                                .read_exact(&mut size)
                                                .await
                                                .map(|_| Size::from_le_bytes(size))
                                                .and_then(|sz| {
                                                    if sz > 0 {
                                                        Ok(sz)
                                                    } else {
                                                        Err(io::Error::from(io::ErrorKind::Other))
                                                    }
                                                })?;

                                            io::Result::Ok(bufs.lock().await.get(sz as usize))
                                        })
                                        .ok()?;

                                    let addr = Size::from_le_bytes(addr);

                                    Some(CTup2(addr, CSliceMut::from(buf)))
                                });

                                let out = &mut |CTup2(addr, buf): RWData| {
                                    let _ = handle.block_on(async {
                                        let mut writer = writer.lock().await;
                                        writer.write_all(&[1]).await?;
                                        writer.write_all(&Size::to_le_bytes(addr)).await?;
                                        writer
                                            .write_all(&Size::to_le_bytes(buf.len() as _))
                                            .await?;
                                        writer.write_all(&buf).await?;
                                        unsafe {
                                            bufs.lock().await.put_back(<&mut [u8]>::from(buf))
                                        };
                                        io::Result::Ok(())
                                    });
                                    true
                                };

                                let out_fail = &mut |fdata: RWFailData| {
                                    let (CTup2(addr, buf), e) = fdata.into();
                                    let _ = handle.block_on(async {
                                        let mut writer = writer.lock().await;
                                        writer.write_all(&[2]).await?;
                                        writer.write_all(&Size::to_le_bytes(addr)).await?;
                                        writer
                                            .write_all(&Size::to_le_bytes(buf.len() as _))
                                            .await?;
                                        writer
                                            .write_all(&e.into_int_err().get().to_le_bytes())
                                            .await?;
                                        unsafe {
                                            bufs.lock().await.put_back(<&mut [u8]>::from(buf))
                                        };
                                        io::Result::Ok(())
                                    });
                                    true
                                };

                                let mut out = out.into();
                                let mut out_fail = out_fail.into();

                                let ops = VecOps {
                                    inp: iter.into(),
                                    out: Some(&mut out),
                                    out_fail: Some(&mut out_fail),
                                };

                                let err = tokio::task::block_in_place(|| node.read(fh, ops));

                                let writer = writer.into_inner();
                                writer.write_all(&[0]).await?;
                                let err = match err {
                                    Ok(_) => 0,
                                    Err(e) => e.into_int_err().get(),
                                };
                                writer.write_all(&err.to_le_bytes()).await?;
                            }
                            Write => {}
                            _ => {}
                        }
                    }

                    /*let n = match reader.read(&mut buf).await {
                        // socket closed
                        Ok(n) if n == 0 => return,
                        Ok(n) => n,
                        Err(e) => {
                            eprintln!("failed to read from socket; err = {:?}", e);
                            return;
                        }
                    };

                    // Write the data back
                    if let Err(e) = writer.write_all(&buf[0..n]).await {
                        eprintln!("failed to write to socket; err = {:?}", e);
                        return;
                    }*/
                }
            });

            self.clients.push((handle, addr));
        }
    }
}
