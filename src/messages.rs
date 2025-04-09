use serde::{Deserialize, Serialize};
use std::clone;
use std::net::{TcpListener, TcpStream};
use std::collections::HashMap;
use std::sync::{Arc,Mutex};

#[derive(Serialize,Deserialize)]
pub enum Hello {
    ClientHello,
    DaemonHello,
    RootHello(Node, u16),
}

#[derive(Serialize,Deserialize)]
pub enum HelloResponse {
    ClientHello(Node),
    DaemonHello,
    RootHello(Node, HashMap<Node, String>),
}

#[derive(Serialize,Deserialize)]
pub enum DaemonRequest {
    Find(String, String),
    Place,
    Read(String),
    Write(String, usize),
    Remove(String),
    AppendDirectoryEntry(String, DirectoryEntry),
}

#[derive(Serialize,Deserialize)]
pub enum DaemonResponse {
    Find(Option<DirectoryEntry>),
    Place(String),
    Read(usize),
    Write(usize),
    Remove(Result<(), ()>),
    AppendDirectoryEntry(Result<(), ()>),
}

#[derive(Serialize,Deserialize)]
pub enum ClientRequest {
    Find(String),
    Place(String, Node),
    Mkdir(String, Node),
    Read(Location),
    Write(Location, usize),
}

#[derive(Serialize,Deserialize)]
pub enum ClientResponse {
    Find(Option<DirectoryEntry>),
    Place(Option<Location>),
    Mkdir(Option<Location>),
    Read(usize),
    Write(usize),
}

#[derive(Debug,Clone,Eq,Hash,PartialEq,Serialize,Deserialize)]
pub struct Location {
    pub node: Node,
    pub uri: String
}

#[derive(Serialize,Deserialize,Clone,Eq,Hash,PartialEq,Debug,Default)]
pub struct Node {
    pub name: String
}

#[derive(Serialize,Deserialize,Clone,Eq,Hash,PartialEq,Debug)]
pub struct DirectoryEntry {
    pub location: Location,
    pub name: String,
    pub is_dir: bool
}
