use bbqueue::BBQueue;

fn main() {
    foo1();
    foo2();
}

fn foo2() {
    let mut bb = BBQueue::new();

    let (mut tx, mut rx) = bb.split();

    // println!("init: {:?}", rx);
    println!("read: {:?}", rx.read());

    println!("\nGRANT 4\n");

    let x = tx.grant(4).unwrap();

    // println!("granted: {:?}", bb);
    println!("read: {:?}", rx.read());

    println!("\nFILL 4\n");

    x.buf.copy_from_slice(&[1, 2, 3, 4]);

    // println!("filled: {:?}", bb);
    println!("read: {:?}", rx.read());

    println!("\nCOMMIT 4\n");

    tx.commit(4, x);

    // println!("committed: {:?}", bb);
    let a = rx.read();
    println!("read: {:?}", a);

    println!("\nRELEASE 4\n");

    rx.release(2, a);

    // println!("released: {:?}", bb);
    println!("read: {:?}", rx.read());

    println!("\nGRANT 2\n");

    let x = tx.grant(2).unwrap();

    // println!("granted: {:?}", bb);
    println!("read: {:?}", rx.read());

    println!("\nFILL 2\n");

    x.buf.copy_from_slice(&[11, 12]);

    // println!("filled: {:?}", bb);
    println!("read: {:?}", rx.read());

    println!("\nCOMMIT 2\n");

    tx.commit(2, x);

    // println!("committed: {:?}", bb);
    let a = rx.read();
    println!("read: {:?}", a);

    println!("\nRELEASE 2\n");

    rx.release(2, a);

    // println!("released: {:?}", bb);
    // println!("read: {:?}", bb.read());

    println!("\nGRANT 3\n");

    let x = tx.grant(3).unwrap();

    // println!("granted: {:?}", bb);
    // println!("read: {:?}", bb.read());

    println!("\nFILL 3\n");

    x.buf.copy_from_slice(&[21, 22, 23]);

    // println!("filled: {:?}", bb);
    println!("read: {:?}", rx.read());

    println!("\nCOMMIT 3\n");

    tx.commit(3, x);

    // println!("committed: {:?}", bb);
    let a = rx.read();
    println!("read: {:?}", a);

    println!("\nRELEASE 2\n");

    rx.release(2, a);

    // println!("released: {:?}", bb);
    println!("read: {:?}", rx.read());

    println!("\nOVERGRANT\n");

    assert!(tx.grant(10).is_err());
}

fn foo1() {
    let mut bb = BBQueue::new();

    println!("init: {:?}", bb);
    println!("read: {:?}", bb.read());

    println!("\nGRANT 4\n");

    let x = bb.grant(4).unwrap();

    println!("granted: {:?}", bb);
    println!("read: {:?}", bb.read());

    println!("\nFILL 4\n");

    x.buf.copy_from_slice(&[1, 2, 3, 4]);

    println!("filled: {:?}", bb);
    println!("read: {:?}", bb.read());

    println!("\nCOMMIT 4\n");

    bb.commit(4, x);

    println!("committed: {:?}", bb);
    let a = bb.read();
    println!("read: {:?}", a);

    println!("\nRELEASE 4\n");

    bb.release(2, a);

    println!("released: {:?}", bb);
    println!("read: {:?}", bb.read());

    println!("\nGRANT 2\n");

    let x = bb.grant(2).unwrap();

    println!("granted: {:?}", bb);
    println!("read: {:?}", bb.read());

    println!("\nFILL 2\n");

    x.buf.copy_from_slice(&[11, 12]);

    println!("filled: {:?}", bb);
    println!("read: {:?}", bb.read());

    println!("\nCOMMIT 2\n");

    bb.commit(2, x);

    println!("committed: {:?}", bb);
    let a = bb.read();
    println!("read: {:?}", a);

    println!("\nRELEASE 2\n");

    bb.release(2, a);

    println!("released: {:?}", bb);
    println!("read: {:?}", bb.read());

    println!("\nGRANT 3\n");

    let x = bb.grant(3).unwrap();

    println!("granted: {:?}", bb);
    println!("read: {:?}", bb.read());

    println!("\nFILL 3\n");

    x.buf.copy_from_slice(&[21, 22, 23]);

    println!("filled: {:?}", bb);
    println!("read: {:?}", bb.read());

    println!("\nCOMMIT 3\n");

    bb.commit(3, x);

    println!("committed: {:?}", bb);
    let a = bb.read();
    println!("read: {:?}", a);

    println!("\nRELEASE 2\n");

    bb.release(2, a);

    println!("released: {:?}", bb);
    println!("read: {:?}", bb.read());

    println!("\nOVERGRANT\n");

    assert!(bb.grant(10).is_err());
}
