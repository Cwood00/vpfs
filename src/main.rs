use vpfs::*;

pub fn main() {
    let vpfs = VPFS::create(7777);

    let vpfs2 = VPFS::connect(7778,"localhost:7777".to_string());
    vpfs2.place("stuff".to_string(),Location{node:Node{addr:"localhost:7778".to_string()},path: "thestuff.txt".to_string()});
    println!("Stuff is at {}",vpfs.find("stuff".to_string()).unwrap().node.addr);    
}