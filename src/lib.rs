use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

pub mod messages;
use messages::*;

pub struct VPFS {
    pub root: Node,
    pub local: Node,
    files: Mutex<HashMap<String, Location>>,
    connections: Mutex<HashMap<Node, TcpStream>>,
}

impl VPFS {
    fn handle_client(vpfs: Arc<VPFS>, mut stream: TcpStream) {
        loop {        
            match Self::receive_request(&mut stream) {       

                Request::Place ( file_name, location ) => {
                    let mut files = vpfs.files.lock().unwrap();
                    files.insert(file_name.to_string(), location.clone());                
                    let response = Response::Place;
                    Self::send_response(&mut stream, response);
                }
                Request::Find ( file_name ) => {
                    let files = vpfs.files.lock().unwrap();
                    let name = file_name.to_string();
                    let location = files.get(&name).cloned();
                    let response = Response::Find(location);
                    Self::send_response(&mut stream, response);
                },
                Request::Read( file_name ) => {
                    let mut reader = std::fs::File::open(file_name).unwrap();
                    let mut buf = vec![];
                    reader.read_to_end(&mut buf).unwrap();
                    Self::send_response(&mut stream, Response::Read(buf.len()));                    
                    stream.write_all(&buf);
                }
                Request::Write( file_name, len) => {
                    let mut writer = std::fs::File::create(file_name).unwrap();
                    let mut buf = vec![0u8;len];
                    stream.read_exact(buf.as_mut()).unwrap();
                    writer.write_all(&buf);
                    Self::send_response(&mut stream, Response::Write(len));
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
        VPFS::connect(listen_port, &format!("localhost:{listen_port}"))
    }

    pub fn stream_for(&self, node: &Node) -> TcpStream {
        let mut connections = self.connections.lock().unwrap();
        if let Some(ref stream) = connections.get(node) {
            stream.try_clone().unwrap()
        }
        else {
            let mut stream = TcpStream::connect(&node.addr).expect("Failed to connect to server");
            connections.insert(node.clone(), stream);
            connections.get(node).unwrap().try_clone().unwrap()
        }
    }

    pub fn connect(listen_port: u16, addr: &str) -> Arc<VPFS> {
        let vpfs = Arc::new(VPFS { 
            root: Node { addr: addr.to_string() },
            local: Node { addr: format!("localhost:{}",listen_port) },
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

    fn send_request_async(&self, node: &Node, req: Request) {
        serde_bare::to_writer(self.stream_for(node), &req).unwrap();
    }
    fn receive_response_async(&self, node: &Node) -> Response {
        let resp = serde_bare::from_reader(self.stream_for(node)).unwrap();
        resp
    }

    fn send_request(&self, node: &Node, req: Request) -> Response {
        let mut stream = self.stream_for(node);
        serde_bare::to_writer(&mut stream, &req).unwrap();
        let resp = serde_bare::from_reader(stream).unwrap();
        resp
    }

    fn receive_request<'a> (stream: &'a mut TcpStream) -> Request {
        serde_bare::from_reader(stream).unwrap()
    }

    fn send_response(stream: &mut TcpStream, resp: Response) {
        serde_bare::to_writer(stream, &resp).unwrap();
    }

    fn receive_response(stream: &mut TcpStream) -> Response {
        serde_bare::from_reader(stream).unwrap()
    }

    pub fn find(&self, name: &str) -> Option<Location> {
        if let Response::Find(loc) = self.send_request(&self.root, Request::Find(name.to_string())) {
            loc
        } else {
            panic!("mismatched response");
        }
    }

    pub fn place(&self, name: &str, at: Location) {
        if let Response::Place = self.send_request(&self.root, Request::Place(name.to_string(), at)) {
        } else {
            panic!("mismatched response");
        }
    }

    pub fn read(&self, what: Location) -> Vec<u8> {
        if let Response::Read(len) = self.send_request(&what.node, Request::Read(what.path)) {
            let mut buf=vec![0u8;len];
            self.connections.lock().unwrap().get(&what.node).unwrap().read_exact(&mut buf);
            buf
        }
        else {
            panic!("bad response to read!");
        }
    } 
    pub fn write(&self, what: Location, buf: &[u8]) {
        self.send_request_async(&what.node, Request::Write(what.path, buf.len()));
        self.connections.lock().unwrap().get(&what.node).unwrap().write(buf);

        if let Response::Write(len) = self.receive_response_async(&what.node) {
            assert!(len == buf.len());
        }
        else {
            panic!("Bad response to write!");
        }

    } 

    fn local_file(&self, name: &str) -> Location {
        Location{node: self.local.clone(), path: name.to_string()}
    }

    pub fn fetch(&self, name: &str) -> Vec<u8> {
        self.read(self.find(name).unwrap())
    }   

    pub fn store(&self, name: &str, buf: &[u8]) {        
        let location = self.local_file(name);
        self.write(location.clone(),buf);
        self.place(name,location);
    }
}