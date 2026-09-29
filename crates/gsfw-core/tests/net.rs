//! The HTTP adapter against a local HTTP server on `127.0.0.1`. No internet is used.
#![cfg(test)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::thread;

use gsfw_core::app::Source as _;
use gsfw_core::net::HttpSource;

/// Serves one request with `status` and `body`; returns the URL.
fn serve_once(status: &'static str, body: &'static [u8]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/file.exe", listener.local_addr().unwrap());
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap() > 2 {
            line.clear();
        }
        let mut stream = reader.into_inner();
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(head.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
    });
    url
}

#[test]
fn a_body_up_to_the_limit_is_returned() {
    let url = serve_once("200 OK", b"abc");
    assert_eq!(HttpSource::new().get(&url, 3), Ok(b"abc".to_vec()));
}

#[test]
fn a_body_over_the_limit_is_an_error() {
    let url = serve_once("200 OK", b"abcd");
    assert!(HttpSource::new().get(&url, 3).is_err());
}

#[test]
fn an_error_status_is_an_error() {
    let url = serve_once("404 Not Found", b"abc");
    assert!(HttpSource::new().get(&url, 3).is_err());
}
