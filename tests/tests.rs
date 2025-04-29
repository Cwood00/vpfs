use vpfs::*;
use vpfs::messages::*;

const LOCAL_PORT: u16 = 8080;
const REMOTE_NAME: &str = "iroh";

#[test]
fn find_and_place_root_directory_remote() {
    let file_name = "test0";
    let root_node = Node {name: REMOTE_NAME.to_string()};

    let vpfs = VPFS::connect(LOCAL_PORT).unwrap();

    let place_ret = vpfs.place(file_name, root_node);
    assert!(place_ret.is_ok());
    let find_ret = vpfs.find(file_name, false);
    assert!(find_ret.is_ok());
    assert_eq!(place_ret.unwrap(), find_ret.unwrap().location);
}

#[test]
fn find_and_place_root_directory_local() {
    let file_name = "test1";

    let vpfs = VPFS::connect(LOCAL_PORT).unwrap();

    let place_ret = vpfs.place(file_name, vpfs.local.clone());
    assert!(place_ret.is_ok());
    let find_ret = vpfs.find(file_name, false);
    assert!(find_ret.is_ok());
    assert_eq!(place_ret.unwrap(), find_ret.unwrap().location);
}

#[test]
fn read_and_write_root_directory_remote() {
    let file_name = "test2";
    let data = "Hello world 2".as_bytes();
    let root_node = Node {name: REMOTE_NAME.to_string()};

    let vpfs = VPFS::connect(LOCAL_PORT).unwrap();

    let location = vpfs.place(file_name, root_node).unwrap();

    vpfs.write(location.clone(), &data);
    assert_eq!(vpfs.read(location).unwrap(), data);
}

#[test]
fn read_and_write_root_directory_local() {
    let file_name = "test3";
    let data = "Hello world 3".as_bytes();

    let vpfs = VPFS::connect(LOCAL_PORT).unwrap();

    let location = vpfs.place(file_name, vpfs.local.clone()).unwrap();

    vpfs.write(location.clone(), &data);
    assert_eq!(vpfs.read(location).unwrap(), data);
}

#[test]
fn store_root_directory_local() {
    let file_name = "test4";
    let data = "Hello world 4".as_bytes();

    let vpfs = VPFS::connect(LOCAL_PORT).unwrap();

    vpfs.store(file_name, data);
    let location = vpfs.find(file_name, false);
    assert!(location.is_ok());
    assert_eq!(vpfs.read(location.unwrap().location).unwrap(), data);
}

#[test]
fn fetch_root_directory_local() {
    let file_name = "test5";
    let data = "Hello world 5".as_bytes();

    let vpfs = VPFS::connect(LOCAL_PORT).unwrap();

    vpfs.store(file_name, data);
    assert_eq!(vpfs.fetch(file_name, false).unwrap(), data);
}

#[test]
fn fetch_root_directory_remote() {
    let file_name = "test6";
    let data = "Hello world 6".as_bytes();
    let root_node = Node {name: REMOTE_NAME.to_string()};

    let vpfs = VPFS::connect(LOCAL_PORT).unwrap();

    let location = vpfs.place(file_name, root_node).unwrap();
    vpfs.write(location, data);
    assert_eq!(vpfs.fetch(file_name, false).unwrap(), data);
}

#[test]
fn find_and_place_non_root_directory_remote() {
    let dir_name = "dir7";
    let file_name = &format!("{dir_name}/test7");
    let root_node = Node {name: REMOTE_NAME.to_string()};

    let vpfs = VPFS::connect(LOCAL_PORT).unwrap();

    let mkdir_ret = vpfs.mkdir(dir_name, root_node.clone());
    assert!(mkdir_ret.is_ok());
    let place_ret = vpfs.place(file_name, root_node);
    assert!(place_ret.is_ok());
    let find_ret = vpfs.find(file_name, false);
    assert!(find_ret.is_ok());
    assert_eq!(place_ret.unwrap(), find_ret.unwrap().location);
}

#[test]
fn find_and_place_non_root_directory_local() {
    let dir_name = "dir8";
    let file_name = &format!("{dir_name}/test8");

    let vpfs = VPFS::connect(LOCAL_PORT).unwrap();

    let mkdir_ret = vpfs.mkdir(dir_name, vpfs.local.clone());
    assert!(mkdir_ret.is_ok());
    let place_ret = vpfs.place(file_name, vpfs.local.clone());
    assert!(place_ret.is_ok());
    let find_ret = vpfs.find(file_name, false);
    assert!(find_ret.is_ok());
    assert_eq!(place_ret.unwrap(), find_ret.unwrap().location);
}

#[test]
fn read_and_write_non_root_directory_remote() {
    let dir_name = "dir9";
    let file_name = &format!("{dir_name}/test9");
    let data = "Hello world 9".as_bytes();
    let root_node = Node {name: REMOTE_NAME.to_string()};

    let vpfs = VPFS::connect(LOCAL_PORT).unwrap();

    let mkdir_ret = vpfs.mkdir(dir_name, root_node.clone());
    assert!(mkdir_ret.is_ok());
    let location = vpfs.place(file_name, root_node).unwrap();

    vpfs.write(location.clone(), &data);
    assert_eq!(vpfs.read(location).unwrap(), data);
}

#[test]
fn read_and_write_non_root_directory_local() {
    let dir_name = "dir10";
    let file_name = &format!("{dir_name}/test10");
    let data = "Hello world 10".as_bytes();

    let vpfs = VPFS::connect(LOCAL_PORT).unwrap();

    let mkdir_ret = vpfs.mkdir(dir_name, vpfs.local.clone());
    assert!(mkdir_ret.is_ok());
    let location = vpfs.place(file_name, vpfs.local.clone()).unwrap();

    vpfs.write(location.clone(), &data);
    assert_eq!(vpfs.read(location).unwrap(), data);
}

#[test]
fn store_non_root_directory_remote() {
    let dir_name = "dir11";
    let file_name = &format!("{dir_name}/test11");
    let data = "Hello world 11".as_bytes();
    let root_node = Node {name: REMOTE_NAME.to_string()};

    let vpfs = VPFS::connect(LOCAL_PORT).unwrap();

    let mkdir_ret = vpfs.mkdir(dir_name, root_node);
    assert!(mkdir_ret.is_ok());
    vpfs.store(file_name, data);
    let location = vpfs.find(file_name, false);
    assert!(location.is_ok());
    assert_eq!(vpfs.read(location.unwrap().location).unwrap(), data);
}

#[test]
fn fetch_non_root_directory_local() {
    let dir_name = "dir12";
    let file_name = &format!("{dir_name}/test12");
    let data = "Hello world 12".as_bytes();
    let root_node = Node {name: REMOTE_NAME.to_string()};

    let vpfs = VPFS::connect(LOCAL_PORT).unwrap();

    let mkdir_ret = vpfs.mkdir(dir_name, root_node);
    assert!(mkdir_ret.is_ok());
    vpfs.store(file_name, data);
    assert_eq!(vpfs.fetch(file_name, false).unwrap(), data);
}

#[test]
fn fetch_non_root_directory_remote() {
    let dir_name = "dir13";
    let file_name = &format!("{dir_name}/test13");
    let data = "Hello world 13".as_bytes();
    let root_node = Node {name: REMOTE_NAME.to_string()};

    let vpfs = VPFS::connect(LOCAL_PORT).unwrap();

    let mkdir_ret = vpfs.mkdir(dir_name, root_node.clone());
    assert!(mkdir_ret.is_ok());
    let location = vpfs.place(file_name, root_node).unwrap();
    vpfs.write(location, data);
    assert_eq!(vpfs.fetch(file_name, false).unwrap(), data);
}

#[test]
fn multiple_nested_directories(){
    let dir_name1 = "dir14";
    let dir_name2 = &format!("{dir_name1}/dir14");
    let dir_name3 = &format!("{dir_name2}/dir14");
    let file_name1 = &format!("{dir_name2}/test14");
    let file_name2 = &format!("{dir_name3}/test14");
    let file_data1 = "First file data".as_bytes();
    let file_data2 = "Second file data".as_bytes();
    let root_node = Node {name: REMOTE_NAME.to_string()};

    let vpfs = VPFS::connect(LOCAL_PORT).unwrap();

    let mkdir_ret = vpfs.mkdir(dir_name1, vpfs.local.clone());
    assert!(mkdir_ret.is_ok());
    let mkdir_ret = vpfs.mkdir(dir_name2, root_node.clone());
    assert!(mkdir_ret.is_ok());
    let mkdir_ret = vpfs.mkdir(dir_name3, vpfs.local.clone());
    assert!(mkdir_ret.is_ok());

    vpfs.store(&file_name1, file_data1);
    assert_eq!(vpfs.fetch(file_name1, false).unwrap(), file_data1);

    let location = vpfs.place(&file_name2, root_node).unwrap();
    vpfs.write(location.clone(), file_data2);
    assert_eq!(vpfs.read(location).unwrap(), file_data2);
}