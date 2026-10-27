//! Explicit socket-free HTTP byte-stream harness; never loopback evidence.
use std::{
    cell::RefCell,
    io,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use zeno_fcis_ingress::{Endpoint, Server, TraceSink, runtime};
struct Bytes {
    input: Vec<u8>,
    position: usize,
    output: Rc<RefCell<Vec<u8>>>,
    stall: bool,
}
impl AsyncRead for Bytes {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let count = buffer.remaining().min(self.input.len() - self.position);
        if count == 0 && self.stall {
            return Poll::Pending;
        }
        buffer.put_slice(&self.input[self.position..self.position + count]);
        self.position += count;
        Poll::Ready(Ok(()))
    }
}
impl AsyncWrite for Bytes {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.output.borrow_mut().extend_from_slice(bytes);
        Poll::Ready(Ok(bytes.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
pub(crate) fn exchange<B: Endpoint, S: TraceSink>(
    server: &mut Server<B, S>,
    input: Vec<u8>,
    stall: bool,
) -> Vec<u8> {
    let output = Rc::new(RefCell::new(Vec::new()));
    let bytes = Bytes {
        input,
        position: 0,
        output: Rc::clone(&output),
        stall,
    };
    let runtime = runtime().unwrap_or_else(|error| panic!("runtime: {error}"));
    let _ = runtime.block_on(server.serve_connection(bytes));
    output.borrow().clone()
}
