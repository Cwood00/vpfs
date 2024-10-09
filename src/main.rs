use vpfs::*;

pub fn main() {
    let vpfs = VPFS::create(7777);

    let vpfs2 = VPFS::connect(7778,"localhost:7777".to_string());
    println!("got here");
    {
    let vpfs2 = vpfs2.lock().unwrap();
    println!("got locked");
    vpfs2.place("stuff".to_string(),Location{node:Node{addr:"localhost:7778".to_string()},path: "thestuff.txt".to_string()});
    }
    println!("got here2");
    println!("Stuff is at {}",vpfs2.lock().unwrap().find("stuff".to_string()).unwrap().node.addr);    
}