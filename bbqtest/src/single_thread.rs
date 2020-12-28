#[cfg(test)]
mod tests {
    use bbqueue::{BBBuffer};

    #[test]
    fn sanity_check() {
        let bb: BBBuffer<6> = BBBuffer::new();
        let (mut prod, mut cons) = bb.try_split().unwrap();

        const ITERS: usize = 100000;

        for i in 0..ITERS {
            let j = (i & 255) as u8;

            #[cfg(feature = "extra-verbose")]
            println!("===========================");
            #[cfg(feature = "extra-verbose")]
            println!("INDEX: {:?}", j);
            #[cfg(feature = "extra-verbose")]
            println!("===========================");

            #[cfg(feature = "extra-verbose")]
            println!("START: {:?}", bb);

            let mut wgr = prod.grant_exact(1).unwrap();

            #[cfg(feature = "extra-verbose")]
            println!("GRANT: {:?}", bb);

            wgr[0] = j;

            #[cfg(feature = "extra-verbose")]
            println!("WRITE: {:?}", bb);

            wgr.commit(1);

            #[cfg(feature = "extra-verbose")]
            println!("COMIT: {:?}", bb);

            let rgr = cons.read().unwrap();

            #[cfg(feature = "extra-verbose")]
            println!("READ : {:?}", bb);

            assert_eq!(rgr[0], j);

            #[cfg(feature = "extra-verbose")]
            println!("RELSE: {:?}", bb);

            rgr.release(1);

            #[cfg(feature = "extra-verbose")]
            println!("FINSH: {:?}", bb);
        }
    }
}
