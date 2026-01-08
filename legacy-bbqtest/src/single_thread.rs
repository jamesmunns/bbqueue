#[cfg(test)]
mod tests {
    use bbqueue::BBBuffer;

    #[test]
    fn sanity_check() {
        let bb: BBBuffer<6> = BBBuffer::new();
        let (mut prod, mut cons) = bb.try_split().unwrap();

        const ITERS: usize = 100000;

        for i in 0..ITERS {
            let j = (i & 255) as u8;

            let mut wgr = prod.grant_exact(1).unwrap();

            wgr[0] = j;

            wgr.commit(1);

            let rgr = cons.read().unwrap();

            assert_eq!(rgr[0], j);

            rgr.release(1);
        }
    }
}
