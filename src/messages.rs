use serde::{Deserialize, Serialize};
use std::clone;
use std::net::{TcpListener, TcpStream};
use std::collections::HashMap;
use std::sync::{Arc,Mutex};

#[derive(Serialize,Deserialize)]
pub enum Request {
    Find(String, String),
    Place,
    Read(String),
    Write(String,usize),
    AppendDirectoryEntry(String, DirectoryEntry)
}

#[derive(Serialize,Deserialize)]
pub enum Response {
    Find(Option<DirectoryEntry>),
    Place(String),
    Read(usize),
    Write(usize),
    AppendDirectoryEntry(bool)
}

#[derive(Debug,Clone,Eq,Hash,PartialEq,Serialize,Deserialize)]
pub struct Location {
    pub node: Node,
    pub uri: String
}

#[derive(Serialize,Deserialize,Clone,Eq,Hash,PartialEq,Debug)]
pub struct Node {
    pub addr: String
}

#[derive(Serialize,Deserialize,Clone,Eq,Hash,PartialEq,Debug)]
pub struct DirectoryEntry {
    pub location: Location,
    pub name: String,
    pub is_dir: bool
}
