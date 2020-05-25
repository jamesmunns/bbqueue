#[cfg(test)]
mod tests {

    use core::convert::TryFrom;
    use core::fmt::Debug;

    use bbqueue::{
        consts::*, ArrayLength, BBBuffer, ConstBBBuffer, Consumer, GrantR, GrantW, Producer,
    };

    enum Potato<'a, T, N>
    where
        T: Sized + TryFrom<usize>,
        N: ArrayLength<T>,
    {
        Tx((Producer<'a, T, N>, u8)),
        Rx((Consumer<'a, T, N>, u8)),
        TxG(GrantW<'a, T, N>),
        RxG(GrantR<'a, T, N>),
        Idle,
        Done,
    }

    #[cfg(not(feature = "short-potato"))]
    const TOTAL_RINGS: usize = 1_000_000;

    #[cfg(feature = "short-potato")]
    const TOTAL_RINGS: usize = 1_000;

    const TX_GRANTS_PER_RING: u8 = 3;
    const RX_GRANTS_PER_RING: u8 = 3;
    const BYTES_PER_GRANT: usize = 129;
    type BufferSize = U4096;

    impl<'a, T, N> Potato<'a, T, N>
    where
        T: Sized + TryFrom<usize> + Debug + PartialEq,
        N: ArrayLength<T>,
    {
        fn work(self) -> (Self, Self) {
            match self {
                Self::Tx((mut prod, ct)) => {
                    // If we are holding a producer, try to send three things before passing it on.
                    if ct == 0 {
                        // If we have exhausted our counts, pass on the sender.
                        (Self::Idle, Self::Tx((prod, TX_GRANTS_PER_RING)))
                    } else {
                        // If we get a grant, pass it on, otherwise keep trying
                        if let Ok(wgr) = prod.grant_exact(BYTES_PER_GRANT) {
                            (Self::Tx((prod, ct - 1)), Self::TxG(wgr))
                        } else {
                            (Self::Tx((prod, ct)), Self::Idle)
                        }
                    }
                }
                Self::Rx((mut cons, ct)) => {
                    // If we are holding a consumer, try to send three things before passing it on.
                    if ct == 0 {
                        // If we have exhausted our counts, pass on the sender.
                        (Self::Idle, Self::Rx((cons, RX_GRANTS_PER_RING)))
                    } else {
                        // If we get a grant, pass it on, otherwise keep trying
                        if let Ok(rgr) = cons.read() {
                            (Self::Rx((cons, ct - 1)), Self::RxG(rgr))
                        } else {
                            (Self::Rx((cons, ct)), Self::Idle)
                        }
                    }
                }
                Self::TxG(mut gr_w) => {
                    gr_w.iter_mut()
                        .take(BYTES_PER_GRANT)
                        .enumerate()
                        .for_each(|(i, by)| {
                            *by = T::try_from(i).ok().expect("can construct from usize")
                        });
                    gr_w.commit(BYTES_PER_GRANT);
                    (Self::Idle, Self::Idle)
                }
                Self::RxG(gr_r) => {
                    gr_r.iter()
                        .take(BYTES_PER_GRANT)
                        .enumerate()
                        .for_each(|(i, by)| {
                            assert_eq!(*by, T::try_from(i).ok().expect("can construct from usize"))
                        });
                    gr_r.release(BYTES_PER_GRANT);
                    (Self::Idle, Self::Idle)
                }
                Self::Idle => (Self::Idle, Self::Idle),
                Self::Done => (Self::Idle, Self::Done),
            }
        }
    }

    // Data type
    type DataTy = u8;
    static BB: BBBuffer<DataTy, BufferSize> = BBBuffer(ConstBBBuffer::new());

    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::thread::spawn;

    #[test]
    fn hello() {
        generic_hello::<DataTy>(&BB);
    }

    fn generic_hello<T>(bb: &'static BBBuffer<T, BufferSize>)
    where
        T: Sized + TryFrom<usize> + Debug + PartialEq,
    {
        let (prod, cons) = bb.try_split().unwrap();

        // create the channels
        let (tx_1_2, rx_1_2): (
            Sender<Potato<'static, T, BufferSize>>,
            Receiver<Potato<'static, T, BufferSize>>,
        ) = channel();
        let (tx_2_3, rx_2_3): (
            Sender<Potato<'static, T, BufferSize>>,
            Receiver<Potato<'static, T, BufferSize>>,
        ) = channel();
        let (tx_3_4, rx_3_4): (
            Sender<Potato<'static, T, BufferSize>>,
            Receiver<Potato<'static, T, BufferSize>>,
        ) = channel();
        let (tx_4_1, rx_4_1): (
            Sender<Potato<'static, T, BufferSize>>,
            Receiver<Potato<'static, T, BufferSize>>,
        ) = channel();

        tx_1_2.send(Potato::Tx((prod, 3))).unwrap();
        tx_1_2.send(Potato::Rx((cons, 3))).unwrap();

        let thread_1 = spawn(move || {
            let mut count = TOTAL_RINGS;
            let mut me: Potato<'static, T, BufferSize> = Potato::Idle;

            loop {
                if let Potato::Idle = me {
                    if let Ok(new) = rx_4_1.recv() {
                        if let Potato::Tx(tx) = new {
                            count -= 1;

                            if (count % 100) == 0 {
                                println!("count left: {}", count);
                            }

                            if count == 0 {
                                me = Potato::Done;
                            } else {
                                me = Potato::Tx(tx);
                            }
                        } else {
                            me = new;
                        }
                    } else {
                        continue;
                    }
                }
                let (new_me, send) = me.work();

                let we_done = if let Potato::Done = &send {
                    true
                } else {
                    false
                };

                let nop = if let Potato::Idle = &send {
                    true
                } else {
                    false
                };

                if !nop {
                    tx_1_2.send(send).unwrap();
                }

                if we_done {
                    println!("We good?");
                    return;
                }

                me = new_me;
            }
        });

        let closure_2_3_4 =
            move |rx: Receiver<Potato<'static, T, BufferSize>>,
                  tx: Sender<Potato<'static, T, BufferSize>>| {
                let mut me: Potato<'static, T, BufferSize> = Potato::Idle;
                let mut count = 0;

                loop {
                    if let Potato::Idle = me {
                        if let Ok(new) = rx.try_recv() {
                            if let Potato::Tx(_) = &new {
                                count += 1;
                            }
                            me = new;
                        } else {
                            continue;
                        }
                    }
                    let (new_me, send) = me.work();

                    let we_done = if let Potato::Done = &send {
                        true
                    } else {
                        false
                    };

                    let nop = if let Potato::Idle = &send {
                        true
                    } else {
                        false
                    };

                    if !nop {
                        tx.send(send).ok();
                    }

                    if we_done {
                        assert_eq!(count, TOTAL_RINGS);
                        println!("We good.");
                        return;
                    }

                    me = new_me;
                }
            };

        let thread_2 = spawn(move || closure_2_3_4(rx_1_2, tx_2_3));
        let thread_3 = spawn(move || closure_2_3_4(rx_2_3, tx_3_4));
        let thread_4 = spawn(move || closure_2_3_4(rx_3_4, tx_4_1));

        thread_1.join().unwrap();
        thread_2.join().unwrap();
        thread_3.join().unwrap();
        thread_4.join().unwrap();
    }
}
