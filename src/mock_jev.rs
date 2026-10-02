use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::Value;

pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

impl Request {
    pub fn header_of(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

pub struct MockJev {
    pub url: String,
    pub requests: Arc<Mutex<Vec<Request>>>,
}

type Respond = dyn Fn(&Request) -> Vec<u8> + Send + Sync;

fn read_request(stream: &TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    reader.read_line(&mut line).ok()?;

    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let mut headers = Vec::new();

    loop {
        line.clear();
        reader.read_line(&mut line).ok()?;

        let header = line.trim_end();

        if header.is_empty() {
            break;
        }

        let (name, value) = header.split_once(':')?;

        headers.push((name.trim().to_string(), value.trim().to_string()));
    }

    let length = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];

    reader.read_exact(&mut body).ok()?;

    Some(Request {
        method,
        path,
        headers,
        body: serde_json::from_slice(&body).unwrap_or(Value::Null),
    })
}

fn response_of(status: u16, headers: Vec<(String, String)>, body: String) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 {status} Status\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );

    for (name, value) in headers {
        response.push_str(&format!("{name}: {value}\r\n"));
    }

    response.push_str("\r\n");
    response.push_str(&body);

    response.into_bytes()
}

fn serve(mut stream: TcpStream, respond: &Respond, requests: &Mutex<Vec<Request>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));

    let Some(request) = read_request(&stream) else {
        return;
    };
    let response = respond(&request);

    requests
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(request);

    let _ = stream.write_all(&response);
    let _ = stream.flush();
}

impl MockJev {
    pub fn start(
        respond: impl Fn(&Request) -> (u16, Vec<(String, String)>, String) + Send + Sync + 'static,
    ) -> MockJev {
        MockJev::start_raw(move |request| {
            let (status, headers, body) = respond(request);

            response_of(status, headers, body)
        })
    }

    pub fn start_raw(respond: impl Fn(&Request) -> Vec<u8> + Send + Sync + 'static) -> MockJev {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);
        let respond: Arc<Respond> = Arc::new(respond);

        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let respond = Arc::clone(&respond);
                let recorded = Arc::clone(&recorded);

                std::thread::spawn(move || serve(stream, &*respond, &recorded));
            }
        });

        MockJev { url, requests }
    }
}
