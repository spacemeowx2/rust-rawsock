/**
    Static dispatch in a loop wih break on demand.
*/

extern crate rawsock;
use rawsock::traits::{DynamicInterface,StaticInterface, Library};
use rawsock::{pcap};
mod commons;
use rawsock::open_best_library;

fn main() {
    let lib = open_best_library().expect("Could not open any packet capturing library");
    let ifname = lib.all_interfaces()
        .expect("Could not obtain interface list").first()
        .expect("There are no available interfaces").name.clone();
    let interf = lib.open_interface(&ifname).expect("Could not open pcap interface");

    let mut count: usize = 0;
    interf.loop_infinite(&mut |packet|{
        count += 1;
        println!("Received packet: {:?}", packet);
        if count >=5 {
            interf.break_loop();
        }
    }).expect("Errow when running receiving loop");
}