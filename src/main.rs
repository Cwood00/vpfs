use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use rand::Rng;
use std::collections::HashMap;
use std::fmt::format;
use std::io::{Read, Write, self};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::os::unix::net::SocketAddr;
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, sleep};
use std::fs;
use std::time::Duration;

mod messages;
use messages::*;

use clap::Parser;

/// A simple example of StructOpt-based CLI parsing
#[derive(Parser, Debug)]
#[command(name = "vpfs", about = "Virtual private file system prototype.")]
struct Opt {
    #[arg(short, long, default_value_t = 8080)]
    port: u16,

    #[arg(short, long)]
    root_addr: Option<String>,

    #[arg(short, long)]
    name: String,

    // When non-zero, artificial latency added to each request
    // Latency specified in milliseconds
    #[arg(short, long, default_value_t = 0)]
    artificial_latency: u64,
}

struct DaemonState {
    root: Option<Node>, // root.is_none() indicates that we are the root
    local: Node,
    connections: Mutex<HashMap<Node, Arc<Mutex<TcpStream>>>>,
    known_hosts: Mutex<Option<HashMap<Node, String>>>,
    cache: (), //TODO
    artificial_latency: Duration,
    file_access_lock: RwLock<()>
}

fn stream_for(node: &Node, state: &Arc<DaemonState>) -> Option<Arc<Mutex<TcpStream>>> {
    let mut connections = state.connections.lock().unwrap();
    if let Some(connection) = connections.get(&node) {
        return Some(connection.clone());
    }
    let known_hosts = state.known_hosts.lock().unwrap();
    if let Some(addr) = known_hosts.as_ref().unwrap().get(&node) {
        let mut stream = Arc::from(Mutex::from(TcpStream::connect(&addr).expect("Failed to connect to server")));
        connections.insert(node.clone(), stream.clone());
        return Some(stream)
    }
    todo!("query root for new connections");
}

fn read_local(uri: &str, buf: &mut Vec<u8>) {
    let mut reader = std::fs::File::open(uri).unwrap();
    let mut buf = vec![];
    reader.read_to_end(&mut buf).unwrap();
}

fn write_local(uri: &str,  data: &Vec<u8>) {
    let mut writer = std::fs::File::create(uri).unwrap();
    writer.write_all(&data);
}

fn handle_client(mut stream: TcpStream, state: Arc<DaemonState>) {
    loop {
        match receive_request(&mut stream, state.artificial_latency) {
            Ok(ClientRequest::Find(file)) => {

            },
            Ok(ClientRequest::Place(file, node )) => {

            }
            Ok(ClientRequest::Mkdir(directory, node )) => {

            }
            Ok(ClientRequest::Read(location)) => {
                if location.node == state.local {
                    let mut buf = vec![];
                    let fs_lock = state.file_access_lock.read().unwrap();
                    read_local(&location.uri, &mut buf);
                    drop(fs_lock);
                    send_message(&mut stream, ClientResponse::Read(buf.len()));                    
                    stream.write_all(&buf);
                }
                else {
                    if let Some(file_owner_connection) = stream_for(&location.node, &state)
                    {
                        let mut file_owner_connection = file_owner_connection.lock().unwrap();
                        send_message(&mut file_owner_connection, DaemonRequest::Read(location.uri));
                        //TODO get length
                        let mut buf = vec![];
                        file_owner_connection.read(&mut buf);
                    }
                    else {
                        todo!("Handle not being abble to connect");
                    }
                }
            }
            Ok(ClientRequest::Write(location,len)) => {
                if location.node == state.local {
                    let mut buf = vec![0u8;len];
                    stream.read_exact(buf.as_mut()).unwrap();
                    let fs_lock = state.file_access_lock.write().unwrap();
                    write_local(&location.uri, &buf);
                    drop(fs_lock);
                    send_message(&mut stream, ClientResponse::Write(len));
                }
                else {

                }
            }
            Err(_) => break
        }
    }
}

fn create_file_with_random_uri() -> String {
    let mut rng = rand::rng();
    let mut uri = format!("{:x}", rng.random::<u64>());
    loop {
        if let Err(error) = fs::File::create_new(&uri) {
            if error.kind() != io::ErrorKind::AlreadyExists {
                panic!("Could not create file"); // TODO better error handeling
            }
            uri = format!("{:x}", rng.random::<u64>());
        }
        else {
            break;
        }
    }
    uri
}

// Assumes caller hold file lock
fn search_directory(file_name: &str, directory_uri: &str) -> Option<DirectoryEntry> {
    let directory_file = fs::File::open(directory_uri).unwrap();
    let mut read_result: Result<DirectoryEntry, serde_bare::error::Error> = serde_bare::from_reader(&directory_file);
    while let Ok(entry) = read_result {
        if entry.name == file_name {
            return Some(entry);
        }
        read_result = serde_bare::from_reader(&directory_file);
    }
    None
}

fn handle_daemon(mut stream: TcpStream, state: Arc<DaemonState>) {
    loop {        
        match receive_request(&mut stream, state.artificial_latency) {
            Ok(DaemonRequest::Place)  => {
                let response = DaemonResponse::Place(create_file_with_random_uri());
                send_message(&mut stream, response);
            }
            Ok(DaemonRequest::Find ( file_name, parent_directory_uri )) => {
                let fs_lock = state.file_access_lock.read().unwrap();
                let response = DaemonResponse::Find(search_directory(&file_name, &parent_directory_uri));
                drop(fs_lock);
                send_message(&mut stream, response);
            },
            Ok(DaemonRequest::Read( uri )) => {
                let mut buf = vec![];
                let fs_lock = state.file_access_lock.read().unwrap();
                read_local( &uri, &mut buf);
                drop(fs_lock);
                send_message(&mut stream, DaemonResponse::Read(buf.len()));                    
                stream.write_all(&buf);
            }
            Ok(DaemonRequest::Write( uri, len)) => {
                let mut buf = vec![0u8;len];
                stream.read_exact(buf.as_mut()).unwrap();
                let fs_lock = state.file_access_lock.write().unwrap();
                write_local(&uri, &buf);
                drop(fs_lock);
                send_message(&mut stream, DaemonResponse::Write(len));
            }
            Ok(DaemonRequest::AppendDirectoryEntry(directory,new_entry )) => {
                // Check if the directory entry already exists
                let fs_lock = state.file_access_lock.write().unwrap();
                if search_directory(&new_entry.name, &directory).is_some() {
                    send_message(&mut stream, DaemonResponse::AppendDirectoryEntry(Err(())));
                    continue;
                }
                let dir_file = fs::OpenOptions::new().append(true).open(directory).unwrap();
                serde_bare::to_writer(dir_file, &new_entry).unwrap();
                drop(fs_lock);
                send_message(&mut stream, DaemonResponse::AppendDirectoryEntry(Ok(())));
            }
            Ok(DaemonRequest::Remove(uri)) => {
                let _fs_lock = state.file_access_lock.write().unwrap();
                if fs::remove_file(uri).is_ok() {
                    send_message(&mut stream, DaemonResponse::Remove(Ok(())));
                } else {
                    send_message(&mut stream, DaemonResponse::Remove(Err(())));
                }
            }
            Err(_) => break
        }
    }
}

fn handel_connection(mut stream: TcpStream, mut state: Arc<DaemonState>) {
    match receive_request(&mut stream, state.artificial_latency) {
        Ok(Hello::ClientHello) => {
            send_message(&mut stream, HelloResponse::ClientHello(state.local.clone()));
            handle_client(stream, state);
        },
        Ok(Hello::DaemonHello(connecting_node, connecting_node_port)) => {
            let conneting_address = match stream.peer_addr().unwrap().ip() {
                std::net::IpAddr::V4(addr) => format!("{}:{}", addr, connecting_node_port),
                std::net::IpAddr::V6(addr) => format!("{}:{}", addr, connecting_node_port),
            };
            {
                let mut known_hosts = state.known_hosts.lock().unwrap();
                known_hosts.as_mut().unwrap().insert(connecting_node, conneting_address);
                send_message(&mut stream, HelloResponse::DaemonHello(state.known_hosts.lock().unwrap().clone().unwrap()));
            }
            handle_daemon(stream, state);
        },
        Err(_) => {eprintln!("Did not recive proper hello message")},
    }
}

// Create a TCP listener to accept incoming connections
fn start_server(address: &str, state: Arc<DaemonState>) {
    let listener = TcpListener::bind(address).unwrap();
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state_clone = state.clone();
                thread::spawn(move || {
                    handel_connection(stream, state_clone); 
                });
            }
            Err(e) => {
                eprintln!("Connection failed: {}", e);
            }
        }
    }
}

fn setup_files_dir() {
    if let Err(err) = fs::create_dir("./files") {
        if err.kind() != std::io::ErrorKind::AlreadyExists {
            panic!("Could not create directory for string files");
        }
    }
    std::env::set_current_dir("./files").expect("Could not cd into ./files directory");
}

fn create_root(listen_port: u16, state: Arc<DaemonState>) {
    setup_files_dir();
    if let Err(create_error) = fs::File::create_new("root") {
        if create_error.kind() != io::ErrorKind::AlreadyExists {
            panic!("Could not create root directory");
        }
    };
    *state.known_hosts.lock().unwrap() = Some(HashMap::new());
    start_server(&format!("0.0.0.0:{listen_port}"), state);
}

fn create(listen_port: u16, state: Arc<DaemonState>, root_addr: String) {
    setup_files_dir();
    let root_connection = TcpStream::connect(root_addr).expect("TODO handel being offline");
    serde_bare::to_writer(&root_connection, &Hello::DaemonHello(state.local.clone(), listen_port)).unwrap();
    if let Ok(HelloResponse::DaemonHello(host_names)) = serde_bare::from_reader(&root_connection,) {
        *state.known_hosts.lock().unwrap() = Some(host_names);
    }
    start_server(&format!("0.0.0.0:{listen_port}"), state);
}

fn receive_request<'a, T: DeserializeOwned> (stream: &'a mut TcpStream, artificial_latency: Duration) -> Result<T, serde_bare::error::Error> {
    let request = serde_bare::from_reader(stream);
    if artificial_latency > Duration::from_millis(0) {
        sleep(artificial_latency);
    }
    request
}

fn send_message <T: Serialize>(stream: &mut TcpStream, message: T) {
    serde_bare::to_writer(stream, &message).unwrap();
}

fn main() {

    let opt = Opt::parse();

    let state = Arc::new(DaemonState {
        root: if opt.root_addr.is_some() {Some(Node{name: opt.name.clone()})} else {None},
        local: Node{name: opt.name},
        connections: Mutex::new(HashMap::new()),
        known_hosts: Mutex::new(None),
        cache: (),
        artificial_latency: Duration::from_millis(opt.artificial_latency),
        file_access_lock: RwLock::new(()),
    });

    if let Some(root_addr) = opt.root_addr{
        println!("running");
        create(opt.port, state, root_addr)
    } else {
        println!("creating");
        create_root(opt.port, state)
    };
}