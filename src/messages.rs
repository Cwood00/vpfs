use serde::{Deserialize, Serialize};
use std::net::{TcpListener, TcpStream};
use std::collections::HashMap;
use std::sync::{Arc,Mutex};

#[derive(Serialize,Deserialize)]
pub enum Request {
    Find(String),
    Place(String,Location),
    Read(String),
    Write(String,usize)
}

#[derive(Serialize,Deserialize)]
pub enum Response {
    Find(Option<Location>),
    Place,
    Read(usize),
    Write(usize)
}

#[derive(Debug,Clone,Eq,Hash,PartialEq,Serialize,Deserialize)]
pub struct Location {
    pub node: Node,
    pub path: String
}

#[derive(Serialize,Deserialize,Clone,Eq,Hash,PartialEq,Debug)]
pub struct Node {
    pub addr: String
}
