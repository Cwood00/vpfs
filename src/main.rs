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
    root: Node,
    local: Node,
    connections: Mutex<HashMap<Node, Arc<Mutex<TcpStream>>>>,
    known_hosts: Mutex<Option<HashMap<Node, String>>>,
    cache: (), //TODO
    artificial_latency: Duration,
    file_access_lock: RwLock<()>
}

/* ------------------------------ Helper functions --------------------------------- */
fn stream_for(node: &Node, state: &Arc<DaemonState>) -> Option<Arc<Mutex<TcpStream>>> {
    let mut connections = state.connections.lock().unwrap();
    if let Some(connection) = connections.get(&node) {
        return Some(connection.clone());
    }
    let known_hosts = state.known_hosts.lock().unwrap();
    if let Some(addr) = known_hosts.as_ref().unwrap().get(&node) {
        let mut stream = TcpStream::connect(&addr).expect("Failed to connect to server");
        send_message(&mut stream, Hello::DaemonHello);
        receive_message::<HelloResponse>(&mut stream, Duration::from_millis(0)).expect("Get bad hello response");
        let stream_arc = Arc::new(Mutex::new(stream));
        connections.insert(node.clone(), stream_arc.clone());
        return Some(stream_arc);
    }
    todo!("query root for new connections");
}

fn receive_message<T: DeserializeOwned> (stream: &mut TcpStream, artificial_latency: Duration) -> Result<T, serde_bare::error::Error> {
    let request = serde_bare::from_reader(stream);
    if artificial_latency > Duration::from_millis(0) {
        sleep(artificial_latency);
    }
    request
}

fn send_message <T: Serialize>(stream: &mut TcpStream, message: T) {
    serde_bare::to_writer(stream, &message).unwrap();
}

fn send_and_recive <T: Serialize, U: DeserializeOwned> (node: &Node, message: T, state: &Arc<DaemonState>) -> Result<U, serde_bare::error::Error> {
    if let Some(node_connection_lock) = stream_for(node, state) {
        let mut node_connection = node_connection_lock.lock().unwrap();
        send_message(&mut node_connection, message);
        receive_message(&mut node_connection, Duration::from_millis(0))
    }
    else {
        todo!("handle not being able to connect");
    }
}

fn read_local(uri: &str, buf: &mut Vec<u8>) {
    let mut reader = std::fs::File::open(uri).unwrap();
    reader.read_to_end(buf).unwrap();
}

fn write_local(uri: &str,  data: &Vec<u8>) {
    let mut writer = std::fs::File::create(uri).unwrap();
    writer.write_all(&data);
}

fn create_file_with_random_uri() -> String {
    let mut rng = rand::rng();
    let mut uri = format!("{:x}", rng.random::<u64>());
    loop {
        if let Err(error) = fs::File::create_new(&uri) {
            if error.kind() != io::ErrorKind::AlreadyExists {
                panic!("Could not create file"); // TODO better error handleing
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
fn search_directory_with_lock(file_name: &str, directory_uri: &str) -> Option<DirectoryEntry> {
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

fn search_directory(file_name: &str, directory_uri: &str, state: &Arc<DaemonState>) -> Option<DirectoryEntry> {
    let _file_access_lock = state.file_access_lock.read().unwrap();
    search_directory_with_lock(file_name, directory_uri)
}

fn append_dir_entry(directory: &str, new_entry: &DirectoryEntry, state: &Arc<DaemonState>) -> Result<(), ()>{
    // Check if the directory entry already exists
    let _fs_lock = state.file_access_lock.write().unwrap();
    if search_directory_with_lock(&new_entry.name, &directory).is_some() {
        Err(())
    }
    else {
        let dir_file = fs::OpenOptions::new().append(true).open(directory).unwrap();
        serde_bare::to_writer(dir_file, &new_entry).unwrap();
        Ok(())
    }
}

/* ------------------- User process connection handler functions ------------------- */
fn recursive_find(file: &str, state: &Arc<DaemonState>) -> Option<DirectoryEntry> {
    if let Some((parent_directory, file_name)) = file.rsplit_once('/') 
    {
        if let Some(parent_directory_entry) = recursive_find(parent_directory, state) {
            if !parent_directory_entry.is_dir {
                return None;
            }
            let parent_dir_node = parent_directory_entry.location.node;
            if parent_dir_node == state.local {
                search_directory(file_name, &parent_directory_entry.location.uri, state)
            }
            else {
                if let Ok(DaemonResponse::Find(entry)) = send_and_recive(&parent_dir_node, DaemonRequest::Find(file_name.to_string(), parent_directory_entry.location.uri), state){
                    entry
                }
                else {
                    panic!("mismatched response");
                }
            }
        }
        else {
            None
        }
    }
    // File is located in the root directory
    else {
        if state.root == state.local {
            search_directory(file, "root", state)
        }
        else if let Ok(DaemonResponse::Find(entry)) = send_and_recive(&state.root, DaemonRequest::Find(file.to_string(), "root".to_string()), state) {
            entry
        }
        else {
            panic!("mismatched response");
        }
    }
}

fn handle_client_find(stream: &mut TcpStream, file: &str, state: &Arc<DaemonState>) {
    send_message(stream, ClientResponse::Find(recursive_find(file, state)));
}

fn place_file(path: &str, at: &Node, is_dir: bool, state: &Arc<DaemonState>) -> Option<Location>{
    let uri = if *at == state.local {
        create_file_with_random_uri()
    }
    else if let Ok(DaemonResponse::Place(uri)) = send_and_recive(at, DaemonRequest::Place, state) {
        uri
    }
    else {
        panic!("Missmatched responce");
    };
    let new_file_location = Location {
        node: at.clone(),
        uri: uri
    };
    let (parent_directory_loaction, file_name) = if let Some((parent_directory, file_name)) = path.rsplit_once('/') {
        if let Some(parent_directory_entry) = recursive_find(parent_directory, state) {
            (parent_directory_entry.location, file_name)
        }
        else {
            todo!("Handle failure to find partent dir directory entry");
        }
    } else {
        (Location {
            node: state.root.clone(),
            uri: "root".to_string()
        },
        path)
    };
    let dir_entry = DirectoryEntry {
        location: new_file_location.clone(),
        name: file_name.to_string(),
        is_dir: is_dir
    };
    let success: Result<(),()>;
    if parent_directory_loaction.node == state.local {
        success = append_dir_entry(&parent_directory_loaction.uri, &dir_entry, state);
        if success.is_ok() && is_dir {
            // TODO handle adding . and .. directory entries
        }
    }
    else if let Ok(DaemonResponse::AppendDirectoryEntry(result)) = send_and_recive(&parent_directory_loaction.node, DaemonRequest::AppendDirectoryEntry(parent_directory_loaction.uri, dir_entry), state){
        success = result;
        if success.is_ok() && is_dir {
            // TODO handle adding . and .. directory entries
        }
    }
    else {
        panic!("Missmatched responce");
    };
    if success.is_err() {
        if *at == state.local {
            fs::remove_file(&new_file_location.uri);
        }
        else {
            send_and_recive::<_, DaemonResponse>(at, DaemonRequest::Remove(new_file_location.uri), state);
        }
        return None;
    }
    
    Some(new_file_location)
}

fn handle_client_place(stream: &mut TcpStream, file: &str, node: Node, state: &Arc<DaemonState>) {
    send_message(stream, ClientResponse::Place(place_file(file, &node, false, state)));
}

fn handle_client_mkdir(stream: &mut TcpStream, directory: &str, node: Node, state: &Arc<DaemonState>) {
    send_message(stream, ClientResponse::Mkdir(place_file(directory, &node, true, state)));
}

fn handle_client_read(stream: &mut TcpStream, location: Location, state: &Arc<DaemonState>) {
    if location.node == state.local {
        let mut buf = vec![];
        let fs_lock = state.file_access_lock.read().unwrap();
        read_local(&location.uri, &mut buf);
        drop(fs_lock);
        send_message(stream, ClientResponse::Read(buf.len()));                    
        stream.write_all(&buf);
    }
    else if let Some(file_owner_connection) = stream_for(&location.node, &state) {
        let mut file_owner_connection = file_owner_connection.lock().unwrap();
        send_message(&mut file_owner_connection, DaemonRequest::Read(location.uri));
        if let Ok(DaemonResponse::Read(file_len)) = receive_message(&mut file_owner_connection, Duration::from_millis(0)) {
            let mut buf = vec![0u8; file_len];
            file_owner_connection.read_exact(&mut buf);
            drop(file_owner_connection);
            send_message(stream, ClientResponse::Read(file_len));
            stream.write_all(&buf);
        }
        else {
            panic!("Bad responce");
        }
    }
    else {
        todo!("Handle not being abble to connect");
    }
}

fn handle_client_write(stream: &mut TcpStream, location: Location, file_len: usize, state: &Arc<DaemonState>) {
    if location.node == state.local {
        let mut buf = vec![0u8;file_len];
        stream.read_exact(buf.as_mut()).unwrap();
        let fs_lock = state.file_access_lock.write().unwrap();
        write_local(&location.uri, &buf);
        drop(fs_lock);
        send_message(stream, ClientResponse::Write(file_len));
    }
    else if let Some(file_owner_connection) = stream_for(&location.node, &state) {
        let mut file_owner_connection = file_owner_connection.lock().unwrap();
        let mut buf = vec![0u8; file_len];
        stream.read_exact(&mut buf);
        send_message(&mut file_owner_connection, DaemonRequest::Write(location.uri, file_len));
        file_owner_connection.write_all(&buf);
        if let Ok(DaemonResponse::Write(bytes_writen)) = receive_message(&mut file_owner_connection, Duration::from_millis(0)) {
            drop(file_owner_connection);
            send_message(stream, ClientResponse::Write(bytes_writen));
        }
    }
    else {
        todo!("Handle not being abble to connect");
    }
}

fn handle_client(mut stream: TcpStream, state: Arc<DaemonState>) {
    loop {
        match receive_message(&mut stream, Duration::from_millis(0)) {
            Ok(ClientRequest::Find(file)) => {
                handle_client_find(&mut stream, &file, &state);
            },
            Ok(ClientRequest::Place(file, node )) => {
                handle_client_place(&mut stream, &file, node,  &state);
            }
            Ok(ClientRequest::Mkdir(directory, node )) => {
                handle_client_mkdir(&mut stream, &directory, node, &state);
            }
            Ok(ClientRequest::Read(location)) => {
                handle_client_read(&mut stream, location, &state);
            }
            Ok(ClientRequest::Write(location,len)) => {
                handle_client_write(&mut stream, location, len, &state);
            }
            Err(_) => break
        }
    }
}

/* ---------------------- Daemon connection handler functions ---------------------- */
fn handle_daemon(mut stream: TcpStream, state: Arc<DaemonState>) {
    loop {        
        match receive_message(&mut stream, state.artificial_latency) {
            Ok(DaemonRequest::Place)  => {
                let response = DaemonResponse::Place(create_file_with_random_uri());
                send_message(&mut stream, response);
            }
            Ok(DaemonRequest::Find ( file_name, parent_directory_uri )) => {
                let fs_lock = state.file_access_lock.read().unwrap();
                let response = DaemonResponse::Find(search_directory(&file_name, &parent_directory_uri, &state));
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
                send_message(&mut stream, DaemonResponse::AppendDirectoryEntry(append_dir_entry(&directory, &new_entry, &state)));
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

/* ------------------------------- Set up functions -------------------------------- */
fn handle_connection(mut stream: TcpStream, state: Arc<DaemonState>) {
    match receive_message(&mut stream, state.artificial_latency) {
        Ok(Hello::ClientHello) => {
            send_message(&mut stream, HelloResponse::ClientHello(state.local.clone()));
            handle_client(stream, state);
        },
        Ok(Hello::DaemonHello) => {
            send_message(&mut stream, HelloResponse::DaemonHello);
            handle_daemon(stream, state);
        }
        Ok(Hello::RootHello(connecting_node, connecting_node_port)) => {
            let conneting_address = match stream.peer_addr().unwrap().ip() {
                std::net::IpAddr::V4(addr) => format!("{}:{}", addr, connecting_node_port),
                std::net::IpAddr::V6(addr) => format!("{}:{}", addr, connecting_node_port),
            };
            {
                let mut known_hosts = state.known_hosts.lock().unwrap();
                known_hosts.as_mut().unwrap().insert(connecting_node, conneting_address);
                send_message(&mut stream, HelloResponse::RootHello(state.root.clone(), state.known_hosts.lock().unwrap().clone().unwrap()));
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
                    handle_connection(stream, state_clone); 
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

fn create_root(listen_port: u16, state: DaemonState) {
    setup_files_dir();
    if let Err(create_error) = fs::File::create_new("root") {
        if create_error.kind() != io::ErrorKind::AlreadyExists {
            panic!("Could not create root directory");
        }
    };
    *state.known_hosts.lock().unwrap() = Some(HashMap::new());
    start_server(&format!("0.0.0.0:{listen_port}"), Arc::new(state));
}

fn create(listen_port: u16, mut state: DaemonState, root_addr: String) {
    setup_files_dir();
    let root_connection = TcpStream::connect(root_addr).expect("TODO handle being offline");
    serde_bare::to_writer(&root_connection, &Hello::RootHello(state.local.clone(), listen_port)).unwrap();
    if let Ok(HelloResponse::RootHello(root_node, host_names)) = serde_bare::from_reader(&root_connection,) {
        *state.known_hosts.lock().unwrap() = Some(host_names);
        state.root = root_node;
    }
    start_server(&format!("0.0.0.0:{listen_port}"), Arc::new(state));
}

fn main() {

    let opt = Opt::parse();

    let state = DaemonState {
        root: if opt.root_addr.is_some() {Default::default()} else {Node{name: opt.name.clone()}},
        local: Node{name: opt.name},
        connections: Mutex::new(HashMap::new()),
        known_hosts: Mutex::new(None),
        cache: (),
        artificial_latency: Duration::from_millis(opt.artificial_latency),
        file_access_lock: RwLock::new(()),
    };

    if let Some(root_addr) = opt.root_addr{
        println!("running");
        create(opt.port, state, root_addr);
    } else {
        println!("creating");
        create_root(opt.port, state);
    };
}