use std::net::UdpSocket;

fn main() -> std::io::Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:34254")?;
    println!("Opened socket at port 34254");
    let mut buf = [0; 1024];

    loop {
        let (amt, src) = socket.recv_from(&mut buf)?;
        println!("Received {amt} bytes from {src:?}");
        println!("{:?}", &buf[0..amt]);
        println!("");
    }
}