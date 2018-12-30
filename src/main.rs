mod bbqueue;
use crate::bbqueue::BBQueue;

fn main() {
    let mut bb = BBQueue::new();

    println!("init: {:?}", bb);
    println!("read: {:?}", bb.read());

    let x = bb.grant(4).unwrap();

    println!("granted: {:?}", bb);
    println!("read: {:?}", bb.read());

    x.buf.copy_from_slice(&[1, 2, 3, 4]);

    println!("filled: {:?}", bb);
    println!("read: {:?}", bb.read());

    bb.commit(4, x);

    println!("committed: {:?}", bb);
    let a = bb.read();
    println!("read: {:?}", a);

    bb.release(2, a);

    println!("released: {:?}", bb);
    println!("read: {:?}", bb.read());

    let x = bb.grant(2).unwrap();

    println!("granted: {:?}", bb);
    println!("read: {:?}", bb.read());

    x.buf.copy_from_slice(&[11, 12]);

    println!("filled: {:?}", bb);
    println!("read: {:?}", bb.read());

    bb.commit(2, x);

    println!("committed: {:?}", bb);
    let a = bb.read();
    println!("read: {:?}", a);

    bb.release(2, a);

    println!("released: {:?}", bb);
    println!("read: {:?}", bb.read());

    let x = bb.grant(3).unwrap();

    println!("granted: {:?}", bb);
    println!("read: {:?}", bb.read());

    x.buf.copy_from_slice(&[21, 22, 23]);

    println!("filled: {:?}", bb);
    println!("read: {:?}", bb.read());

    bb.commit(3, x);

    println!("committed: {:?}", bb);
    let a = bb.read();
    println!("read: {:?}", a);

    bb.release(2, a);

    println!("released: {:?}", bb);
    println!("read: {:?}", bb.read());

    assert!(bb.grant(10).is_err());
}
