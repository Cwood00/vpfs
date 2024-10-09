use serde::{Deserialize, Serialize};
use std::net::{TcpListener, TcpStream};
use std::collections::HashMap;
use std::sync::{Arc,Mutex};

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