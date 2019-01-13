#[cfg(test)]
mod tests {
    use bbqueue::{
        BBQueue,
        bbq,
    };

    // AJM: This test hangs/fails!
    #[test]
    fn sanity_check() {
        let bb = bbq!(6).unwrap();

        const ITERS: usize = 100000;

        for i in 0..ITERS {
            let j = (i & 255) as u8;

            // eprintln!("===========================");
            // eprintln!("INDEX: {:?}", j);
            // eprintln!("===========================");

            // eprintln!("START: {:?}", bb);

            let mut wgr = bb.grant(1).unwrap();

            // eprintln!("GRANT: {:?}", bb);

            wgr[0] = j;

            // eprintln!("WRITE: {:?}", bb);

            bb.commit(1, wgr);

            // eprintln!("COMIT: {:?}", bb);

            let rgr = bb.read().unwrap();

            // eprintln!("READ : {:?}", bb);

            assert_eq!(rgr[0], j);

            // eprintln!("RELSE: {:?}", bb);

            bb.release(1, rgr);

            // eprintln!("FINSH: {:?}", bb);
        }
    }
}
