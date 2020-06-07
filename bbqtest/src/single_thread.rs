#[cfg(test)]
mod tests {
    use core::convert::TryFrom;
    use std::fmt::Debug;

    use bbqueue::{consts::*, GenericBBBuffer};

    #[test]
    fn sanity_check_u8() {
        generic_sanity_check::<u8>();
    }

    #[test]
    fn sanity_check_pod() {
        generic_sanity_check::<U8Sized>();
        generic_sanity_check::<PodStruct>();
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct U8Sized(u8);

    impl From<u8> for U8Sized {
        fn from(v: u8) -> Self {
            U8Sized(v)
        }
    }

    // TODO: tailor this to address specific error cases if any
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct PodStruct {
        array: [u8; 32],
        variants: PodEnum,
    }

    impl From<u8> for PodStruct {
        fn from(v: u8) -> Self {
            PodStruct {
                array: [v; 32],
                variants: PodEnum::from(v),
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum PodEnum {
        Empty,
        Tuple((u64, i64, usize)),
        Array([i16; 16]),
    }

    impl From<u8> for PodEnum {
        fn from(v: u8) -> Self {
            if v == 0 {
                Self::Empty
            } else if v < 128 {
                Self::Tuple((v as u64, (v + 1) as i64, (v + 2) as usize))
            } else {
                Self::Array([v as i16; 16])
            }
        }
    }

    #[test]
    fn sanity_check_ptr() {
        generic_sanity_check::<RefStruct>();
    }

    #[derive(Debug, Clone, PartialEq)]
    struct RefStruct {
        vec: Vec<u8>,
        maybe_boxed: Option<Box<RefStruct>>,
    }

    impl From<u8> for RefStruct {
        fn from(v: u8) -> Self {
            let mut vec = Vec::with_capacity(v as usize);
            vec.iter_mut().for_each(|e| *e = v);

            let maybe_boxed = Some(Box::new(RefStruct {
                vec: vec.clone(),
                maybe_boxed: None,
            }));
            RefStruct { vec, maybe_boxed }
        }
    }

    fn generic_sanity_check<T>()
    where
        T: Sized + TryFrom<u8> + Debug + PartialEq + Clone,
    {
        let bb: GenericBBBuffer<T, U6> = GenericBBBuffer::new();
        let (mut prod, mut cons) = bb.try_split().unwrap();

        const ITERS: usize = 100000;

        for i in 0..ITERS {
            let j = T::try_from((i & 255) as u8)
                .ok()
                .expect("can construct type from u8");

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

            wgr[0] = j.clone();

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
