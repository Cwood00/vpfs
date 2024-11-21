use vpfs::*;
use vpfs::messages::*;
use std::time::*;
use clap::Parser;

/// A simple example of StructOpt-based CLI parsing
#[derive(Parser, Debug)]
#[command(name = "vpfs", about = "Virtual private file system prototype.")]
struct Opt {
    #[arg(short, long, default_value_t = 8080)]
    port: u16,

    #[arg(short, long)]
    root: bool,

    #[arg(short, long)]
    connect_to: Option<String>
}


pub fn main() {

    let opt = Opt::parse();

    let vpfs = if opt.root { 
        println!("creating");
         VPFS::create(opt.port)
    } else {
        VPFS::connect(opt.port,&opt.connect_to.unwrap())
    };


    if !opt.root {
        vpfs.place("mystuff.txt",Location{node: vpfs.root.clone(), path: "mystuff.txt".to_string()});
        vpfs.write(vpfs.find("mystuff.txt").unwrap(),"Hello world one!".as_bytes());
//        println!("Stored at: {:?} Content was: {:?}",vpfs.find("mystuff.txt"),String::from_utf8(vpfs.fetch("mystuff.txt")));

//        println!("Stored at: {:?}",vpfs.find("mystuff.txt"));
        vpfs.store("mystuff.txt","Hello World again!".as_bytes());
//        println!("Stored at: {:?} Content was: {:?}",vpfs.find("mystuff.txt"),String::from_utf8(vpfs.fetch("mystuff.txt")));


        vpfs.place("mystuff.txt",Location{node: vpfs.root.clone(), path: "mystuff.txt".to_string()});
        let l = vpfs.find("mystuff.txt").unwrap();
        for s in 0..27 {
            let data=vec![s as u8;(2usize).pow(s)];
            let start = Instant::now();
            vpfs.write(l.clone(),&data);
            println!("Time taken for remote write operation {}: {:?}", s, start.elapsed());
        }
        vpfs.store("mystuff.txt","Hello World again2!".as_bytes());
        for s in 0..27 {
            let data=vec![s as u8;(2usize).pow(s)];
            let start = Instant::now();
            vpfs.store("mystuff.txt",&data);
            println!("Time taken for local write operation {}: {:?}", s, start.elapsed());
        }

    }
    loop{}
}