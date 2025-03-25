use serde::{Deserialize, Serialize};
use rand::Rng;
use std::fmt::format;
use std::io::{Read, Write, self};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
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
    root: bool,

    // When non-zero, artificial latency added to each request
    // Latency specified in milliseconds
    #[arg(short, long, default_value_t = 0)]
    artificial_latency: u64,
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

fn handle_client(mut stream: TcpStream, artificial_latency: u64) {
    loop {        
        match receive_request(&mut stream, artificial_latency) {
            Ok(Request::Place)  => {
                let response = Response::Place(create_file_with_random_uri());
                send_response(&mut stream, response);
            }
            Ok(Request::Find ( file_name, parent_directory_uri )) => {
                let response = Response::Find(search_directory(&file_name, &parent_directory_uri));
                send_response(&mut stream, response);
            },
            Ok(Request::Read( uri )) => {
                let mut reader = std::fs::File::open(uri).unwrap();
                let mut buf = vec![];
                reader.read_to_end(&mut buf).unwrap();
                send_response(&mut stream, Response::Read(buf.len()));                    
                stream.write_all(&buf);
            }
            Ok(Request::Write( file_name, len)) => {
                let mut writer = std::fs::File::create(file_name).unwrap();
                let mut buf = vec![0u8;len];
                stream.read_exact(buf.as_mut()).unwrap();
                writer.write_all(&buf);
                send_response(&mut stream, Response::Write(len));
            }
            Ok(Request::AppendDirectoryEntry(directory,new_entry )) => {
                // Check if the directory entry already exists
                if search_directory(&new_entry.name, &directory).is_some() {
                    send_response(&mut stream, Response::AppendDirectoryEntry(Err(())));
                    continue;
                }
                let dir_file = fs::OpenOptions::new().append(true).open(directory).unwrap();
                serde_bare::to_writer(dir_file, &new_entry).unwrap();
                send_response(&mut stream, Response::AppendDirectoryEntry(Ok(())));
            }
            Ok(Request::Remove(uri)) => {
                if fs::remove_file(uri).is_ok() {
                    send_response(&mut stream, Response::Remove(Ok(())));
                } else {
                    send_response(&mut stream, Response::Remove(Err(())));
                }
            }
            Err(_) => break
        }
    }
}

// Create a TCP listener to accept incoming connections
fn start_server(address: &str, artificial_latency: u64) {
    let listener = TcpListener::bind(address).unwrap();
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    handle_client(stream, artificial_latency); 
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
    std::env::set_current_dir("./files").unwrap();
}

pub fn create_root(listen_port: u16, artificial_latency: u64) {
    setup_files_dir();
    if let Err(create_error) = fs::File::create_new("root") {
        if create_error.kind() != io::ErrorKind::AlreadyExists {
            panic!("Could not create root directory");
        }
    };
    start_server(&format!("0.0.0.0:{listen_port}"), artificial_latency);
}

pub fn create(listen_port: u16, artificial_latency: u64) {
    setup_files_dir();
    start_server(&format!("0.0.0.0:{listen_port}"), artificial_latency);
}

fn receive_request<'a> (stream: &'a mut TcpStream, artificial_latency: u64) -> Result<Request, serde_bare::error::Error> {
    let request = serde_bare::from_reader(stream);
    if artificial_latency > 0 {
        sleep(Duration::from_millis(artificial_latency));
    }
    request
}

fn send_response(stream: &mut TcpStream, resp: Response) {
    serde_bare::to_writer(stream, &resp).unwrap();
}

pub fn main() {

    let opt = Opt::parse();

    if opt.root { 
        println!("creating");
        create_root(opt.port, opt.artificial_latency)
    } else {
        println!("running");
        create(opt.port, opt.artificial_latency)
    };
}