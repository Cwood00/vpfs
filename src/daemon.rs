use std::collections::HashMap;
use std::io::{Read, Write, BufReader, self};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::thread::{self, sleep};
use std::fs;
use std::time::Duration;

use clap::Parser;
use lru::LruCache;
use serde::de::DeserializeOwned;
use serde::ser::Error;
use serde::{Deserialize, Serialize};
use rand::Rng;

mod messages;
use messages::*;


/// A simple example of StructOpt-based CLI parsing
#[derive(Parser, Debug)]
#[command(name = "vpfs", about = "Virtual private file system prototype.")]
struct Opt {
    #[arg(short, long, default_value_t = 8080)]
    port: u16,

    #[arg(short, long)]
    root_addr: Option<String>,

    #[arg(short, long)]
    listening_addr: Option<String>,

    #[arg(short, long)]
    name: String,

    //Maximum cache size in bytes
    #[arg(short, long, default_value_t = 1 << 16)]
    cache_size: usize,

    // When non-zero, artificial latency added to each request
    // Latency specified in milliseconds
    #[arg(short, long, default_value_t = 0)]
    artificial_latency: u64,
}

struct DaemonState {
    root: Option<Node>,
    local: Node,
    connections: Mutex<HashMap<Node, Arc<Mutex<TcpStream>>>>,
    known_hosts: Mutex<Option<HashMap<Node, String>>>,
    cache: Mutex<LruCache<Location, CacheEntry>>,
    max_cache_size: usize,
    used_cache_bytes: RwLock<usize>,
    artificial_latency: Duration,
    file_access_lock: RwLock<()>
}

/* ------------------------------ Helper functions --------------------------------- */
fn establish_connecttion(node: &Node, connections: &mut MutexGuard<HashMap<Node, Arc<Mutex<TcpStream>>>>, addr: &str) -> Option<Arc<Mutex<TcpStream>>> {
    if let Ok(mut stream) = TcpStream::connect(&addr) {
        send_message(&mut stream, Hello::DaemonHello);
        receive_message::<HelloResponse>(&mut stream).expect("Got bad hello response");
        let stream_arc = Arc::new(Mutex::new(stream));
        connections.insert(node.clone(), stream_arc.clone());
        Some(stream_arc)
    }
    else {
        None
    }
}

fn stream_for(node: &Node, state: &Arc<DaemonState>) -> Option<Arc<Mutex<TcpStream>>> {
    let mut connections = state.connections.lock().unwrap();
    if let Some(connection) = connections.get(&node) {
        return Some(connection.clone());
    }
    let known_hosts = state.known_hosts.lock().unwrap();
    if let Some(addr) = known_hosts.as_ref().unwrap().get(&node) {
        return establish_connecttion(node, &mut connections, addr);
    }
    if let Some(root_node) = &state.root {
        if state.local == *root_node {
            return None;
        }
        if let Some(root_connection) = connections.get(root_node) {
            let mut root_connection = root_connection.lock().unwrap();
            send_message(&mut root_connection, DaemonRequest::AddressFor(node.clone()));
            match receive_message(&mut root_connection) {
                Ok(DaemonResponse::AddressFor(Some(addr))) => {
                    drop(root_connection);
                    return establish_connecttion(node, &mut connections, &addr)
                },
                _ => return None
            }
        }
    }
    None
}

fn receive_message_with_latceny<T: DeserializeOwned>(stream: &mut TcpStream, artificial_latency: Duration) -> Result<T, serde_bare::error::Error> {
    let request = serde_bare::from_reader(stream);
    if artificial_latency > Duration::from_millis(0) {
        sleep(artificial_latency);
    }
    request
}

fn receive_message<T: DeserializeOwned> (stream: &mut TcpStream) -> Result<T, serde_bare::error::Error> {
    receive_message_with_latceny(stream, Duration::from_millis(0))
}

fn send_message <T: Serialize>(stream: &mut TcpStream, message: T) {
    serde_bare::to_writer(stream, &message).unwrap();
}

fn send_and_recive <T: Serialize, U: DeserializeOwned> (node: &Node, message: T, state: &Arc<DaemonState>) -> Result<U, serde_bare::error::Error> {
    if let Some(node_connection_lock) = stream_for(node, state) {
        let mut node_connection = node_connection_lock.lock().unwrap();
        send_message(&mut node_connection, message);
        receive_message(&mut node_connection)
    }
    else {
        Err(serde_bare::error::Error::custom("Could not connect"))
    }
}

fn read_local(uri: &str, fs_lock: &RwLock<()>) -> io::Result<Vec<u8>>{
    fs_lock.read().unwrap();
    fs::read(uri)
}

fn read_remote(location: &Location, state: &Arc<DaemonState>) -> Result<Vec<u8>, VPFSError> {
    let mut cache = state.cache.lock().unwrap();
    let cache_entry = cache.get(&location);
    let _fs_lock = state.file_access_lock.write().unwrap();
    let cache_last_update_time = if let Some(cache_entry) = cache_entry {
        if let Ok(file_data) = fs::metadata(&cache_entry.uri) {
            file_data.modified().ok()
        }
        else {
            None
        }
    }
    else {
        None
    };
    if let Some(file_owner_connection) = stream_for(&location.node, state) {
        let mut file_owner_connection = file_owner_connection.lock().unwrap();
        send_message(&mut file_owner_connection, DaemonRequest::Read(location.uri.clone(), cache_last_update_time));

        match receive_message(&mut file_owner_connection) {
            Ok(DaemonResponse::Read(Ok(file_len))) => {
                let mut buf = vec![0u8; file_len];
                file_owner_connection.read_exact(&mut buf);

                add_cache_entry(location, &buf, &mut cache, state);

                Ok(buf)
            },
            Ok(DaemonResponse::Read(Err(VPFSError::NotModified))) => {
                Ok(fs::read(&cache_entry.unwrap().uri).expect("Missing file for cache entry"))
            }
            Ok(DaemonResponse::Read(Err(error))) => {
                Err(error)
            },
            Ok(_) => panic!("Bad responce"),
            Err(_) => {
                todo!("Check if error came from bad responce, or from connection closing")
            }
        }
    }
    else {
        if let Some(cache_entry) =  cache_entry{
            let cache_entry_location = Location {
                node: state.local.clone(),
                uri: cache_entry.uri.clone()
            };
            Err(VPFSError::OnlyInCache(cache_entry_location))
        }
        else {
            Err(VPFSError::NotAccessible)
        }
    }
}

fn write_local(uri: &str,  data: &Vec<u8>, fs_lock: &RwLock<()>) -> io::Result<()>{
    fs_lock.write().unwrap();
    if fs::exists(uri)? {
        fs::write(uri, data)
    }
    else {
        Err(io::Error::from(io::ErrorKind::NotFound))
    }
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

fn search_directory_with_reader<T: Read>(file_name: &str, directory_reader: &mut T) -> Result<DirectoryEntry, VPFSError> {
    let mut read_result: Result<DirectoryEntry, serde_bare::error::Error> = serde_bare::from_reader(&mut *directory_reader);
    let mut dir_entry = Err(VPFSError::DoesNotExist);
    while let Ok(entry) = read_result {
        if entry.name == file_name{
            dir_entry = Ok(entry);
            break;
        }
        read_result = serde_bare::from_reader(&mut *directory_reader);
    }
    dir_entry
}

//Assumes caller hold file lock
fn search_directory_with_lock(file_name: &str, directory_uri: &str) -> Result<DirectoryEntry, VPFSError> {
    let mut directory_file = fs::File::open(directory_uri).unwrap();
    search_directory_with_reader(file_name, &mut directory_file)
}

fn search_directory(file_name: &str, directory_uri: &str, state: &Arc<DaemonState>) -> Result<DirectoryEntry, VPFSError> {
    let _file_access_lock = state.file_access_lock.read().unwrap();
    search_directory_with_lock(file_name, directory_uri)
}

fn append_dir_entry(directory: &str, new_entry: &DirectoryEntry, state: &Arc<DaemonState>) -> Result<(), VPFSError>{
    // Check if the directory entry already exists
    let _fs_lock = state.file_access_lock.write().unwrap();
    if let Ok(existing_dir_entry) = search_directory_with_lock(&new_entry.name, &directory) {
        Err(VPFSError::AlreadyExists(existing_dir_entry))
    }
    else {
        let dir_file = fs::OpenOptions::new().append(true).open(directory).unwrap();
        serde_bare::to_writer(dir_file, &new_entry).unwrap();
        Ok(())
    }
}

fn add_cache_entry(location: &Location, data: &[u8], cache: &mut MutexGuard<LruCache<Location, CacheEntry>>, state: &Arc<DaemonState>) {
    if let Some(cache_entry) = cache.get(&location) {
        fs::write(&cache_entry.uri, &data);
    }
    else {
        let new_cache_entry = CacheEntry {
            uri: create_file_with_random_uri(),
        };
        fs::write(&new_cache_entry.uri, &data);
        cache.put(location.clone(), new_cache_entry);
    };
    let mut used_cache = state.used_cache_bytes.write().unwrap();
    *used_cache += data.len();
    // Evict elements to make room in cache
    while *used_cache > state.max_cache_size {
        if let Some((_, lru_entry)) = cache.pop_lru() {
            let file_size = fs::metadata(&lru_entry.uri).expect("Cache entry missing backing file").len();
            fs::remove_file(&lru_entry.uri).unwrap();
            *used_cache -= file_size as usize;
        }
        else {
            break;
        }
    }
    let cache_file = fs::File::create("cache").expect("Failed to create cache file");
    serde_bare::to_writer(&cache_file, &state.root).expect("Failed to save root node to file");
    serde_bare::to_writer(&cache_file, &*used_cache).expect("Failed to save cahce size to file");
    for (key, value) in cache.iter() {
        serde_bare::to_writer(&cache_file, key).expect("Could not write cache entry to file");
        serde_bare::to_writer(&cache_file, value).expect("Could not write cache entry to file");
    }
}

fn restore_cache(state: &mut DaemonState) {
    if let Ok(cache_file) = fs::File::open("cache") {
        let mut cache = state.cache.lock().unwrap();
        state.root = serde_bare::from_reader(&cache_file).expect("Failed to readed from cache file");
        state.used_cache_bytes = serde_bare::from_reader(&cache_file).expect("Failed to readed from cache file");
        while let Ok(key) = serde_bare::from_reader::<_, Location>(&cache_file) {
            let value = serde_bare::from_reader(&cache_file).unwrap();
            cache.put(key.clone(), value);
            cache.demote(&key);
        }
    }
}

/* ------------------- User process connection handler functions ------------------- */
fn recursive_find(file: &str, state: &Arc<DaemonState>) -> Result<DirectoryEntry, VPFSError> {
    if let Some((parent_directory, file_name)) = file.rsplit_once('/') 
    {
        match recursive_find(parent_directory, state) {
            Ok(parent_dir_entry) => {
                if !parent_dir_entry.is_dir {
                    return Err(VPFSError::NotADirectory);
                }
                if parent_dir_entry.location.node == state.local {
                    search_directory(file_name, &parent_dir_entry.location.uri, state)
                }
                else {
                    match read_remote(&parent_dir_entry.location, state) {
                        Ok(directory) => search_directory_with_reader(file_name, &mut BufReader::new(&*directory)),
                        Err(VPFSError::OnlyInCache(cache_location)) => {
                            let dir_entry = search_directory(file_name, &cache_location.uri, state);
                            if let Ok(dir_entry) = dir_entry {
                                Err(VPFSError::CacheNeededForTraversal(dir_entry))
                            }
                            else {
                                dir_entry
                            }
                        },
                        Err(error) => Err(error)
                    }
                }
            },
            Err(VPFSError::CacheNeededForTraversal(parent_dir_entry)) => {
                if !parent_dir_entry.is_dir {
                    return Err(VPFSError::NotADirectory);
                }
                if parent_dir_entry.location.node == state.local {
                    let dir_entry = search_directory(file_name, &parent_dir_entry.location.uri, state);
                    if let Ok(dir_entry) = dir_entry {
                        Err(VPFSError::CacheNeededForTraversal(dir_entry))
                    }
                    else {
                        dir_entry
                    }
                }
                else {
                    match read_remote(&parent_dir_entry.location, state) {
                        Ok(directory) => {
                            let dir_entry = search_directory_with_reader(file_name, &mut BufReader::new(&*directory));
                            if let Ok(dir_entry) = dir_entry {
                                Err(VPFSError::CacheNeededForTraversal(dir_entry))
                            }
                            else {
                                dir_entry
                            }
                        },
                        Err(VPFSError::OnlyInCache(cache_location)) => {
                            let dir_entry = search_directory(file_name, &cache_location.uri, state);
                            if let Ok(dir_entry) = dir_entry {
                                Err(VPFSError::CacheNeededForTraversal(dir_entry))
                            }
                            else {
                                dir_entry
                            }
                        },
                        Err(error) => Err(error)
                    }
                }
            }
            error => error
        }
    }
    // Base case, file is located in the root directory
    else if let Some(root_node) = &state.root{
        if *root_node == state.local {
            search_directory(file, "root", state)
        }
        else {
            let root_location = Location{
                node: root_node.clone(),
                uri: "root".to_string()
            };
            match read_remote(&root_location, state) {
                Ok(root_dir) => search_directory_with_reader(file, &mut BufReader::new(&*root_dir)),
                Err(VPFSError::OnlyInCache(cache_location)) => {
                    let dir_entry = search_directory(file, &cache_location.uri, state);
                    if let Ok(dir_entry) = dir_entry {
                        Err(VPFSError::CacheNeededForTraversal(dir_entry))
                    }
                    else {
                        dir_entry
                    }
                },
                Err(error) => Err(error)
            }
        }
    }
    else {
        Err(VPFSError::NotAccessible)
    }
}

fn handle_client_find(stream: &mut TcpStream, file: &str, state: &Arc<DaemonState>) {
    send_message(stream, ClientResponse::Find(recursive_find(file, state)));
}

fn place_file(path: &str, at: &Node, is_dir: bool, state: &Arc<DaemonState>) -> Result<Location, VPFSError>{
    let uri = if *at == state.local {
        create_file_with_random_uri()
    }
    else if let Ok(DaemonResponse::Place(uri)) = send_and_recive(at, DaemonRequest::Place, state) {
        uri
    }
    else {
        return Err(VPFSError::NotAccessible);
    };
    let new_file_location = Location {
        node: at.clone(),
        uri: uri
    };
    let parent_directory_loaction;
    let file_name;
    if let Some((parent_directory, _file_name)) = path.rsplit_once('/') {
        let parent_directory_entry = recursive_find(parent_directory, state)?;
        parent_directory_loaction = parent_directory_entry.location;
        file_name = _file_name;
    } 
    else if let Some(root_node) = &state.root{
        parent_directory_loaction = Location {
            node: root_node.clone(),
            uri: "root".to_string()
        };
        file_name = path
    }
    else {
        return Err(VPFSError::NotAccessible);
    }
    let mut dir_entry = DirectoryEntry {
        location: new_file_location.clone(),
        name: file_name.to_string(),
        is_dir: is_dir
    };

    let success= if parent_directory_loaction.node == state.local {
        append_dir_entry(&parent_directory_loaction.uri, &dir_entry, state)
    }
    else {
        match send_and_recive(&parent_directory_loaction.node, DaemonRequest::AppendDirectoryEntry(parent_directory_loaction.uri.clone(), dir_entry.clone()), state){
            Ok(DaemonResponse::AppendDirectoryEntry(result)) => result,
            Ok(_) => Err(VPFSError::Other("Bad response".to_string())),
            Err(error) => Err(VPFSError::Other("Connection closed".to_string()))
        }
    };
    // Add . and .. directory entries if new file is a directory
    if success.is_ok() && is_dir {
        let dot_dot_entry = DirectoryEntry {
            location: parent_directory_loaction.clone(),
            name: "..".to_string(),
            is_dir: true,
        };
        dir_entry.name = ".".to_string();
        if *at == state.local {
            let _ = append_dir_entry(&new_file_location.uri, &dir_entry, state);
            let _ = append_dir_entry(&new_file_location.uri, &dot_dot_entry, state);
        }
        else {
            send_and_recive::<_, DaemonResponse>(at, DaemonRequest::AppendDirectoryEntry(new_file_location.uri.clone(), dir_entry), state);
            send_and_recive::<_, DaemonResponse>(at, DaemonRequest::AppendDirectoryEntry(new_file_location.uri.clone(), dot_dot_entry), state);
        }
    }
    else if let Err(error) = success {
        if *at == state.local {
            fs::remove_file(&new_file_location.uri);
        }
        else {
            send_and_recive::<_, DaemonResponse>(at, DaemonRequest::Remove(new_file_location.uri), state);
        }
        return Err(error);
    }
    
    Ok(new_file_location)
}

fn handle_client_place(stream: &mut TcpStream, file: &str, node: Node, state: &Arc<DaemonState>) {
    send_message(stream, ClientResponse::Place(place_file(file, &node, false, state)));
}

fn handle_client_mkdir(stream: &mut TcpStream, directory: &str, node: Node, state: &Arc<DaemonState>) {
    send_message(stream, ClientResponse::Mkdir(place_file(directory, &node, true, state)));
}

fn handle_client_read(stream: &mut TcpStream, location: Location, state: &Arc<DaemonState>) {
    if location.node == state.local {
        if let Ok(buf) = read_local(&location.uri, &state.file_access_lock) {
            send_message(stream, ClientResponse::Read(Ok(buf.len())));                    
            stream.write_all(&buf);
        }
        else {
            send_message(stream, ClientResponse::Read(Err(VPFSError::DoesNotExist)));
        }
    }
    else  { 
        match read_remote(&location, state) {
            Ok(buf) => {
                send_message(stream, ClientResponse::Read(Ok(buf.len())));                    
                stream.write_all(&buf);
            }
            Err(error) => {
                send_message(stream, ClientResponse::Read(Err(error)));
            }
        }
    }
}

fn handle_client_write(stream: &mut TcpStream, location: Location, file_len: usize, state: &Arc<DaemonState>) {
    if location.node == state.local {
        let mut buf = vec![0u8;file_len];
        stream.read_exact(buf.as_mut()).unwrap();
        if write_local(&location.uri, &buf, &state.file_access_lock).is_ok() {
            send_message(stream, ClientResponse::Write(Ok(file_len)));
        }
        else {
            send_message(stream, ClientResponse::Write(Err(VPFSError::DoesNotExist)));
        }
    }
    else if let Some(file_owner_connection) = stream_for(&location.node, &state) {
        let mut file_owner_connection = file_owner_connection.lock().unwrap();
        let mut buf = vec![0u8; file_len];
        stream.read_exact(&mut buf);
        send_message(&mut file_owner_connection, DaemonRequest::Write(location.uri, file_len));
        file_owner_connection.write_all(&buf);
        if let Ok(DaemonResponse::Write(write_result)) = receive_message(&mut file_owner_connection) {
            drop(file_owner_connection);
            send_message(stream, ClientResponse::Write(write_result));
        }
    }
    else {
        send_message(stream, ClientResponse::Write(Err(VPFSError::NotAccessible)));
    }
}

fn handle_client(mut stream: TcpStream, state: Arc<DaemonState>) {
    loop {
        match receive_message(&mut stream) {
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
            Err(_) => {
                println!("Client diconnected");
                break;
            }
        }
    }
}

/* ---------------------- Daemon connection handler functions ---------------------- */
fn handle_daemon(mut stream: TcpStream, state: Arc<DaemonState>) {
    loop {        
        match receive_message_with_latceny(&mut stream, state.artificial_latency) {
            Ok(DaemonRequest::Place)  => {
                let response = DaemonResponse::Place(create_file_with_random_uri());
                send_message(&mut stream, response);
            }
            Ok(DaemonRequest::Read( uri, last_modified )) => {
                if let Some(remote_last_modified) = last_modified {
                    let fs_lock = state.file_access_lock.read().unwrap();
                    if let Ok(file_data) = fs::metadata(&uri) {
                        if let Ok(local_last_modified) = file_data.modified() {
                            if local_last_modified < remote_last_modified {
                                drop(fs_lock);
                                send_message(&mut stream, DaemonResponse::Read(Err(VPFSError::NotModified)));
                                continue;
                            }
                        }
                    }
                }
                if let Ok(buf) = read_local(&uri, &state.file_access_lock) {
                    send_message(&mut stream, DaemonResponse::Read(Ok(buf.len())));                    
                    stream.write_all(&buf);
                }
                else {
                    send_message(&mut stream, DaemonResponse::Read(Err(VPFSError::DoesNotExist)));
                }
            }
            Ok(DaemonRequest::Write( uri, len)) => {
                let mut buf = vec![0u8;len];
                stream.read_exact(buf.as_mut()).unwrap();
                if write_local(&uri, &buf, &state.file_access_lock).is_ok() {
                    send_message(&mut stream, DaemonResponse::Write(Ok(len)));
                }
                else {
                    send_message(&mut stream, DaemonResponse::Write(Err(VPFSError::DoesNotExist)));
                }
            }
            Ok(DaemonRequest::AppendDirectoryEntry(directory,new_entry )) => {
                send_message(&mut stream, DaemonResponse::AppendDirectoryEntry(append_dir_entry(&directory, &new_entry, &state)));
            }
            Ok(DaemonRequest::Remove(uri)) => {
                let _fs_lock = state.file_access_lock.write().unwrap();
                if fs::remove_file(uri).is_ok() {
                    send_message(&mut stream, DaemonResponse::Remove(Ok(())));
                } else {
                    send_message(&mut stream, DaemonResponse::Remove(Err(VPFSError::DoesNotExist)));
                }
            }
            Ok(DaemonRequest::AddressFor(node)) => {
                let known_hosts_lock = state.known_hosts.lock().unwrap();
                if let Some(known_hosts) = known_hosts_lock.as_ref() {
                    send_message(&mut stream, DaemonResponse::AddressFor(known_hosts.get(&node).map(|entry| -> String {entry.clone()})));
                }
                send_message(&mut stream, DaemonResponse::AddressFor(None));
            }
            Err(_) => {
                println!("Daemon dissconnected");
                break;
            }
        }
    }
}

/* ------------------------------- Set up functions -------------------------------- */
fn handle_connection(mut stream: TcpStream, state: Arc<DaemonState>) {
    match receive_message_with_latceny(&mut stream, state.artificial_latency) {
        Ok(Hello::ClientHello) => {
            println!("User process connected");
            send_message(&mut stream, HelloResponse::ClientHello(state.local.clone()));
            handle_client(stream, state);
        },
        Ok(Hello::DaemonHello) => {
            println!("Daemon process connected");
            send_message(&mut stream, HelloResponse::DaemonHello);
            handle_daemon(stream, state);
        }
        Ok(Hello::RootHello(connecting_node, connecting_address)) => {
            println!("Daemon process connected to root, is listening on {connecting_address}");
            {
                let mut known_hosts = state.known_hosts.lock().unwrap();
                known_hosts.as_mut().unwrap().insert(connecting_node, connecting_address);
                send_message(&mut stream, HelloResponse::RootHello(state.root.clone().unwrap(), known_hosts.clone().unwrap()));
            }
            handle_daemon(stream, state);
        },
        Err(_) => {eprintln!("Did not recive proper hello message")},
    }
}

// Create a TCP listener to accept incoming connections
fn start_server(address: &str, state: Arc<DaemonState>) {
    let listener = TcpListener::bind(address).unwrap();
    println!("Listening for connections");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("Incomming connections");
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
            panic!("Could not create directory for storing files");
        }
    }
    std::env::set_current_dir("./files").expect("Could not cd into ./files directory");
}

fn create_root(listen_port: u16, mut state: DaemonState) {
    setup_files_dir();
    restore_cache(&mut state);
    *state.known_hosts.lock().unwrap() = Some(HashMap::new());
    let state_arc = Arc::new(state);
    if let Err(create_error) = fs::File::create_new("root") {
        if create_error.kind() != io::ErrorKind::AlreadyExists {
            panic!("Could not create root directory");
        }
    }
    else {
        let mut self_link = DirectoryEntry {
            location: Location { node: state_arc.local.clone(), uri: "root".to_string() },
            name: ".".to_string(),
            is_dir: true
        };
        let _ = append_dir_entry("root", &self_link, &state_arc);
        self_link.name = "..".to_string();
        let _ = append_dir_entry("root", &self_link, &state_arc);
    }
    start_server(&format!("0.0.0.0:{listen_port}"), state_arc);
}

fn create(listen_port: u16, mut state: DaemonState, root_addr: String, listening_addr: String) {
    setup_files_dir();
    restore_cache(&mut state);
    if let Ok(root_connection) = TcpStream::connect(&root_addr) {
        serde_bare::to_writer(&root_connection, &Hello::RootHello(state.local.clone(), listening_addr)).unwrap();
        if let Ok(HelloResponse::RootHello(root_node, host_names)) = serde_bare::from_reader(&root_connection,) {
            let mut known_hosts = state.known_hosts.lock().unwrap();
            *known_hosts = Some(host_names);
            known_hosts.as_mut().unwrap().insert(root_node.clone(), root_addr);
            state.root = Some(root_node);
        }
        else {
            panic!("Bad hello reponce");
        }
    }
    start_server(&format!("0.0.0.0:{listen_port}"), Arc::new(state));
}

fn main() {

    let opt = Opt::parse();

    let state = DaemonState {
        root: if opt.root_addr.is_some() {None} else {Some(Node{name: opt.name.clone()})},
        local: Node{name: opt.name},
        connections: Mutex::new(HashMap::new()),
        known_hosts: Mutex::new(None),
        cache: Mutex::new(LruCache::unbounded()),
        max_cache_size: opt.cache_size,
        used_cache_bytes: RwLock::new(0),
        artificial_latency: Duration::from_millis(opt.artificial_latency),
        file_access_lock: RwLock::new(()),
    };

    if let Some(root_addr) = opt.root_addr{
        println!("running");
        if let Some(listening_addr) = opt.listening_addr {
            create(opt.port, state, root_addr, listening_addr);
        }
        else {
            println!("Must specify address for other daemons to connect to");
        }
        
    } else {
        println!("creating");
        create_root(opt.port, state);
    };
}