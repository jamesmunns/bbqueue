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

            #[cfg(feature = "extra-verbose")] println!("===========================");
            #[cfg(feature = "extra-verbose")] println!("INDEX: {:?}", j);
            #[cfg(feature = "extra-verbose")] println!("===========================");

            #[cfg(feature = "extra-verbose")] println!("START: {:?}", bb);

            let mut wgr = bb.grant(1).unwrap();

            #[cfg(feature = "extra-verbose")] println!("GRANT: {:?}", bb);

            wgr[0] = j;

            #[cfg(feature = "extra-verbose")] println!("WRITE: {:?}", bb);

            bb.commit(1, wgr);

            #[cfg(feature = "extra-verbose")] println!("COMIT: {:?}", bb);

            let rgr = bb.read().unwrap();

            #[cfg(feature = "extra-verbose")] println!("READ : {:?}", bb);

            assert_eq!(rgr[0], j);

            #[cfg(feature = "extra-verbose")] println!("RELSE: {:?}", bb);

            bb.release(1, rgr);

            #[cfg(feature = "extra-verbose")] println!("FINSH: {:?}", bb);
        }
    }
}
