## Virtual Private File System (VPFS)

VPFS attempts to make a novel file system abstractions, where files present on different devices are presented in a single directory hierarchy. However, the reality of where files are located and their current availability is exposed to the
user.

## Running the program

This repository compiles to two binaries. The first binary is the VPFS daemon, which handles all accesses to VPFS files on the local device, as well as forwarding requests from user processes running on the local machine to other machines as needed. The second is a shell program meant to serve as an example program that utilizes VPFS. The shell utilizes the API made available through the `src/lib.rs` file. Other programs may be linked with this file to utilize the API.

### VPFS daemon

The daemon can be optionally run in root mode. When running in root mode, the daemon has the additional responsibilities of managing the root directory and providing hosts with the information needed to connect to each other. It is expected the exactly one host in a system runs as the root node. The daemon can be run using `cargo run --bin daemon -- -n <name> [additional options]`. By default the daemon runs in root mode. Options that can be specified when running the daemon are:

`-n <name>` The name you want to give the current machine in the system. The name must be unique, and should always be the same on the same device.

`-p <port>` The port number that the daemon should listen on. Default value: `8080`.

`-r <addr>` Specifies that the current node should not act as the root node, and should try the find the root node at the provided address. Addresses should be of the form `address:port_number`.

`-l <addr>` Specifies the addresses that other devices can uses to connect to the local machine. Must be provided if `-r` is specified. Addresses should be of the form `address:port_number`.

`-c <cache_size>` The size of the local machine's cache in bytes. Default value: `65,536`.

`-a <latency>` Artificial latency on remote requests in milliseconds, used for testing. Default value: `0`.

### VPFS Shell

To run the shell you must have an instance of the VPFS daemon running on the same machine. The shell currently supports the following built commands for interacting with the VPFS system `cd`, `pwd`, `mkdir`, `ls`, and `exit`. All commands work the same as their Unix counter parts, but may only take a limited subset of the normally supported arguments. Other commands can interact with the VPFS system by using I/O redirection with `<` and `>`. The shell can be run with `cargo run --bin sh [-- options]`. Options that can be specified when running the shell are:

`-p <port>` The port number that the VPFS daemon running on the local machine is listening on. Default value: `8080`.

