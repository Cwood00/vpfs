use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

pub mod messages;

#[derive(Serialize,Deserialize)]
enum Request {
    Find(String),
    Place(String,Location)
}

#[derive(Serialize,Deserialize)]
enum Response {
    Find(Option<Location>),
    Success
}

#[derive(Clone,Eq,Hash,PartialEq,Serialize,Deserialize)]
pub struct Location {
    pub node: Node,
    pub path: String
}

#[derive(Serialize,Deserialize,Clone,Eq,Hash,PartialEq)]
pub struct Node {
    pub addr: String
}

pub struct VPFS {
    root: Node,
    files: Mutex<HashMap<String, Location>>,
    connections: Mutex<HashMap<Node, TcpStream>>,
}

impl VPFS {
    fn handle_client(vpfs: Arc<VPFS>, mut stream: TcpStream) {
        loop {        
            match Self::receive_request(&mut stream) {            
                Request::Place ( file_name, location ) => {
                    let mut files = vpfs.files.lock().unwrap();
                    files.insert(file_name.clone(), location);                
                    let response = Response::Success;
                    Self::send_response(&mut stream, response);
                }
                Request::Find ( file_name ) => {
                    let files = vpfs.files.lock().unwrap();
                    let location = files.get(&file_name).cloned();
                    let response = Response::Find(location);
                    Self::send_response(&mut stream, response);
                }
            }
        }
    }

    // Create a TCP listener to accept incoming connections
    fn start_server(vpfs: Arc<VPFS>, address: &str) {
        let listener = TcpListener::bind(address).unwrap();
        thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let vpfs2 = vpfs.clone();
                        thread::spawn(move || {
                            VPFS::handle_client(vpfs2, stream); 
                        });
                    }
                    Err(e) => {
                        eprintln!("Connection failed: {}", e);
                    }
                }
            }
        });
    }

    pub fn create(listen_port: u16) -> Arc<VPFS> {
        VPFS::connect(listen_port, "localhost:7777".to_string())
    }

    pub fn connect(listen_port: u16, addr: String) -> Arc<VPFS> {
        let vpfs = Arc::new(VPFS { 
            root: Node { addr: addr.clone() },
            files: Mutex::new(Default::default()),
            connections: Mutex::new(Default::default()),
        });
        VPFS::start_server(vpfs.clone(), &format!("0.0.0.0:{listen_port}"));

        {
            let mut stream = TcpStream::connect(addr).expect("Failed to connect to server");
            let root = vpfs.root.clone();
            let mut connections = vpfs.connections.lock().unwrap();
            connections.insert(root, stream);
        }
        vpfs
    }

    fn send_request(&self, node: Node, req: Request) -> Response {
        let connections = self.connections.lock().unwrap();
        let stream = connections.get(&node).unwrap();
        serde_bare::to_writer(stream, &req).unwrap();
        let resp = serde_bare::from_reader(stream).unwrap();
        resp
    }

    fn receive_request(stream: &mut TcpStream) -> Request {
        serde_bare::from_reader(stream).unwrap()
    }

    fn send_response(stream: &mut TcpStream, resp: Response) {
        serde_bare::to_writer(stream, &resp).unwrap();
    }

    fn receive_response(stream: &mut TcpStream) -> Response {
        serde_bare::from_reader(stream).unwrap()
    }

    pub fn find(&self, name: String) -> Option<Location> {
        if let Response::Find(loc) = self.send_request(self.root.clone(), Request::Find(name)) {
            loc
        } else {
            panic!("mismatched response");
        }
    }

    pub fn place(&self, name: String, at: Location) {
        if let Response::Success = self.send_request(self.root.clone(), Request::Place(name, at)) {
        } else {
            panic!("mismatched response");
        }
    }
}