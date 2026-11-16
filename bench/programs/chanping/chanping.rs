use std::sync::mpsc::sync_channel;
use std::thread;

fn main() {
    let n: i64 = std::env::args().nth(1).unwrap().parse().unwrap();
    // Capacity 0: a rendezvous, as an unbuffered channel is.
    let (ping_tx, ping_rx) = sync_channel::<i64>(0);
    let (pong_tx, pong_rx) = sync_channel::<i64>(0);
    let echo = thread::spawn(move || {
        for v in ping_rx {
            pong_tx.send(v + 1).unwrap();
        }
    });
    let mut v = 0;
    for _ in 0..n {
        ping_tx.send(v).unwrap();
        v = pong_rx.recv().unwrap();
    }
    drop(ping_tx);
    echo.join().unwrap();
    println!("{v}");
}
