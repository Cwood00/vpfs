use serde::{Deserialize, Serialize};
use std::clone;
use std::net::{TcpListener, TcpStream};
use std::collections::HashMap;
use std::sync::{Arc,Mutex};
use std::time::SystemTime;

#[derive(Serialize,Deserialize)]
pub enum Hello {
    ClientHello,
    DaemonHello,
    RootHello(Node, String),
}

#[derive(Serialize,Deserialize)]
pub enum HelloResponse {
    ClientHello(Node),
    DaemonHello,
    RootHello(Node, HashMap<Node, String>),
}

#[derive(Serialize,Deserialize)]
pub enum DaemonRequest {
    Place,
    Read(String, Option<SystemTime>),
    Write(String, usize),
    Remove(String),
    AppendDirectoryEntry(String, DirectoryEntry),
}

#[derive(Serialize,Deserialize)]
pub enum DaemonResponse {
    Place(String),
    Read(Result<usize, VPFSError>),
    Write(Result<usize, VPFSError>),
    Remove(Result<(), VPFSError>),
    AppendDirectoryEntry(Result<(), VPFSError>),
}

#[derive(Serialize,Deserialize)]
pub enum ClientRequest {
    Find(String, bool),
    Place(String, Node),
    Mkdir(String, Node),
    Read(Location, bool),
    Write(Location, usize),
}

#[derive(Serialize,Deserialize)]
pub enum ClientResponse {
    Find(Result<DirectoryEntry, VPFSError>),
    Place(Result<Location, VPFSError>),
    Mkdir(Result<Location, VPFSError>),
    Read(Result<usize, VPFSError>),
    Write(Result<usize, VPFSError>),
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

pub struct CacheEntry {
    pub uri: String
}

#[derive(Serialize,Deserialize,Debug,Eq,PartialEq)]
pub enum VPFSError {
    OnlyInCache,
    NotModified,
    DoesNotExist, // We can verify that the file does not exist
    NotFound,     // We can not find the file. File may or may not exist
    NotADirectory,
    AlreadyExists,
    CouldNotPlaceAtNode,
    Other
}