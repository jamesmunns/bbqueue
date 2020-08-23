#[cfg(test)]
mod tests {
    use bbqueue::{consts::*, BBBuffer};

    #[test]
    fn frame_wrong_size() {
        let bb: BBBuffer<U256> = BBBuffer::new();
        let (mut prod, mut cons) = bb.try_split_framed().unwrap();

        // Create largeish grants
        let mut wgr = prod.grant(127).unwrap();
        for (i, by) in wgr.iter_mut().enumerate() {
            *by = i as u8;
        }
        // Note: In debug mode, this hits a debug_assert
        wgr.commit(256);

        let rgr = cons.read().unwrap();
        assert_eq!(rgr.len(), 127);
        for (i, by) in rgr.iter().enumerate() {
            assert_eq!((i as u8), *by);
        }
        rgr.release();
    }

    #[test]
    fn full_size() {
        let bb: BBBuffer<U256> = BBBuffer::new();
        let (mut prod, mut cons) = bb.try_split_framed().unwrap();
        let mut ctr = 0;

        for _ in 0..10_000 {
            // Create largeish grants
            if let Ok(mut wgr) = prod.grant(127) {
                ctr += 1;
                for (i, by) in wgr.iter_mut().enumerate() {
                    *by = i as u8;
                }
                wgr.commit(127);

                let rgr = cons.read().unwrap();
                assert_eq!(rgr.len(), 127);
                for (i, by) in rgr.iter().enumerate() {
                    assert_eq!((i as u8), *by);
                }
                rgr.release();
            } else {
                // Create smallish grants
                let mut wgr = prod.grant(1).unwrap();
                for (i, by) in wgr.iter_mut().enumerate() {
                    *by = i as u8;
                }
                wgr.commit(1);

                let rgr = cons.read().unwrap();
                assert_eq!(rgr.len(), 1);
                for (i, by) in rgr.iter().enumerate() {
                    assert_eq!((i as u8), *by);
                }
                rgr.release();
            };
        }

        assert!(ctr > 1);
    }

    #[test]
    fn frame_overcommit() {
        let bb: BBBuffer<U256> = BBBuffer::new();
        let (mut prod, mut cons) = bb.try_split_framed().unwrap();

        // Create largeish grants
        let mut wgr = prod.grant(128).unwrap();
        for (i, by) in wgr.iter_mut().enumerate() {
            *by = i as u8;
        }
        wgr.commit(255);

        let mut wgr = prod.grant(64).unwrap();
        for (i, by) in wgr.iter_mut().enumerate() {
            *by = (i as u8) + 128;
        }
        wgr.commit(127);

        let rgr = cons.read().unwrap();
        assert_eq!(rgr.len(), 128);
        rgr.release();

        let rgr = cons.read().unwrap();
        assert_eq!(rgr.len(), 64);
        rgr.release();
    }

    #[test]
    fn frame_undercommit() {
        let bb: BBBuffer<U512> = BBBuffer::new();
        let (mut prod, mut cons) = bb.try_split_framed().unwrap();

        for _ in 0..100_000 {
            // Create largeish grants
            let mut wgr = prod.grant(128).unwrap();
            for (i, by) in wgr.iter_mut().enumerate() {
                *by = i as u8;
            }
            wgr.commit(13);

            let mut wgr = prod.grant(64).unwrap();
            for (i, by) in wgr.iter_mut().enumerate() {
                *by = (i as u8) + 128;
            }
            wgr.commit(7);

            let mut wgr = prod.grant(32).unwrap();
            for (i, by) in wgr.iter_mut().enumerate() {
                *by = (i as u8) + 192;
            }
            wgr.commit(0);

            let rgr = cons.read().unwrap();
            assert_eq!(rgr.len(), 13);
            rgr.release();

            let rgr = cons.read().unwrap();
            assert_eq!(rgr.len(), 7);
            rgr.release();

            let rgr = cons.read().unwrap();
            assert_eq!(rgr.len(), 0);
            rgr.release();
        }
    }

    #[test]
    fn frame_auto_commit_release() {
        let bb: BBBuffer<U256> = BBBuffer::new();
        let (mut prod, mut cons) = bb.try_split_framed().unwrap();

        for _ in 0..100 {
            {
                let mut wgr = prod.grant(64).unwrap().into_auto_commit();
                for (i, by) in wgr.iter_mut().enumerate() {
                    *by = i as u8;
                }
                // drop
            }

            {
                let rgr = cons.read().unwrap().into_auto_release();
                for (i, by) in rgr.iter().enumerate() {
                    assert_eq!(*by, i as u8);
                }
                assert_eq!(rgr.len(), 64);
                // drop
            }
        }

        assert!(cons.read().is_none());
    }
}
